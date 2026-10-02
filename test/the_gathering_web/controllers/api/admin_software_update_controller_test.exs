defmodule TheGatheringWeb.API.AdminSoftwareUpdateControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  import Ecto.Query
  import TheGathering.AccountsFixtures

  alias TheGathering.Accounts.UserToken
  alias TheGathering.Repo
  alias TheGathering.SelfUpdate

  setup do
    dir = Path.join(System.tmp_dir!(), "software-update-#{System.unique_integer([:positive])}")
    File.mkdir_p!(dir)
    previous = Application.get_env(:the_gathering, SelfUpdate, [])
    SelfUpdate.reset()

    Req.Test.stub(__MODULE__, fn conn ->
      Req.Test.json(conn, %{
        "tag_name" => "v9.9.9",
        "html_url" => "https://github.com/cfbender/the-gathering/releases/tag/v9.9.9"
      })
    end)

    Req.Test.allow(__MODULE__, self(), Process.whereis(SelfUpdate))

    on_exit(fn ->
      Application.put_env(:the_gathering, SelfUpdate, previous)
      SelfUpdate.reset()
      File.rm_rf!(dir)
    end)

    %{dir: dir}
  end

  defp configure(dir, options) do
    version_file = Path.join(dir, "VERSION")
    File.write!(version_file, "v0.1.0\n")

    Application.put_env(
      :the_gathering,
      SelfUpdate,
      [version_file: version_file, req_options: [plug: {Req.Test, __MODULE__}]] ++ options
    )
  end

  test "status and requests need an administrator with recent authentication", %{dir: dir} do
    configure(dir, request_file: Path.join(dir, "update-request"))
    admin = admin_fixture()
    member = user_fixture()
    stale = log_in_user(build_conn(), admin)
    token = get_session(stale, :user_token)

    Repo.update_all(from(t in UserToken, where: t.token == ^token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-11, :minute) |> DateTime.truncate(:second)
      ]
    )

    for method <- [:get, :post] do
      assert build_conn() |> request(method) |> json_response(401)
      assert build_conn() |> log_in_user(member) |> request(method) |> json_response(403)

      assert %{"errors" => %{"code" => "sudo_required"}} =
               stale |> request(method) |> json_response(403)
    end

    refute File.exists?(Path.join(dir, "update-request"))
  end

  test "reports the version and hands the update to systemd", %{dir: dir} do
    request_file = Path.join(dir, "update-request")
    configure(dir, request_file: request_file)
    admin = admin_fixture()

    status_conn = build_conn() |> log_in_user(admin) |> get("/api/admin/software-update")

    assert %{
             "data" => %{
               "version" => "v0.1.0",
               "channel" => "release",
               "method" => "systemd",
               "pending" => false,
               "requested_at" => nil,
               "update_available" => true,
               "check_error" => nil,
               "latest" => %{
                 "version" => "v9.9.9",
                 "url" => "https://github.com/cfbender/the-gathering/releases/tag/v9.9.9"
               }
             }
           } = json_response(status_conn, 200)

    assert get_resp_header(status_conn, "cache-control") == ["no-store"]

    assert %{"data" => %{"pending" => true, "requested_at" => requested_at}} =
             build_conn()
             |> log_in_user(admin)
             |> post("/api/admin/software-update")
             |> json_response(202)

    assert {:ok, %DateTime{}, 0} = DateTime.from_iso8601(requested_at)
    assert File.exists?(request_file)
  end

  test "refuses when no updater is configured", %{dir: dir} do
    configure(dir, [])
    admin = admin_fixture()

    assert %{"data" => %{"method" => nil}} =
             build_conn()
             |> log_in_user(admin)
             |> get("/api/admin/software-update")
             |> json_response(200)

    assert %{"errors" => %{"detail" => "Bad Request"}} =
             build_conn()
             |> log_in_user(admin)
             |> post("/api/admin/software-update")
             |> json_response(400)
  end

  test "maps Watchtower outcomes to 409 and 502", %{dir: dir} do
    configure(dir, watchtower_token: "secret")
    admin = admin_fixture()

    Req.Test.expect(__MODULE__, 1, fn conn ->
      conn |> Plug.Conn.put_status(429) |> Req.Test.json(%{"error" => "another update"})
    end)

    assert build_conn()
           |> log_in_user(admin)
           |> post("/api/admin/software-update")
           |> json_response(409)

    Req.Test.expect(__MODULE__, 1, fn conn -> Req.Test.transport_error(conn, :econnrefused) end)

    ExUnit.CaptureLog.capture_log(fn ->
      assert build_conn()
             |> log_in_user(admin)
             |> post("/api/admin/software-update")
             |> json_response(502)
    end)
  end

  defp request(conn, :get), do: get(conn, "/api/admin/software-update")
  defp request(conn, :post), do: post(conn, "/api/admin/software-update")
end
