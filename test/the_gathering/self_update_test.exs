defmodule TheGathering.SelfUpdateTest do
  use ExUnit.Case, async: false

  import ExUnit.CaptureLog

  alias TheGathering.SelfUpdate

  @nightly_sha "0123456789abcdef0123456789abcdef01234567"

  setup do
    dir = Path.join(System.tmp_dir!(), "self-update-#{System.unique_integer([:positive])}")
    File.mkdir_p!(dir)
    previous = Application.get_env(:the_gathering, SelfUpdate, [])
    SelfUpdate.reset()
    # The GenServer makes the requests; it may use this test's stubs once one exists to own.
    stub_github(%{})
    Req.Test.allow(__MODULE__, self(), Process.whereis(SelfUpdate))

    on_exit(fn ->
      Application.put_env(:the_gathering, SelfUpdate, previous)
      SelfUpdate.reset()
      File.rm_rf!(dir)
    end)

    %{dir: dir, version_file: Path.join(dir, "VERSION")}
  end

  defp configure(%{version_file: version_file}, version, options) do
    if version, do: File.write!(version_file, version <> "\n")

    Application.put_env(
      :the_gathering,
      SelfUpdate,
      [version_file: version_file, req_options: [plug: {Req.Test, __MODULE__}]] ++ options
    )
  end

  defp stub_github(responses) do
    Req.Test.stub(__MODULE__, fn conn ->
      case Map.fetch(responses, conn.request_path) do
        {:ok, {status, body}} -> conn |> Plug.Conn.put_status(status) |> Req.Test.json(body)
        {:ok, body} -> Req.Test.json(conn, body)
        :error -> conn |> Plug.Conn.put_status(404) |> Req.Test.json(%{})
      end
    end)
  end

  describe "status/0" do
    test "a development build has no version, channel, or updater and skips the check", ctx do
      configure(ctx, nil, [])
      Req.Test.stub(__MODULE__, fn _conn -> flunk("GitHub must not be called") end)

      assert %{
               version: nil,
               channel: nil,
               method: nil,
               pending: false,
               latest: nil,
               update_available: nil,
               check_error: nil
             } = SelfUpdate.status()
    end

    test "compares a tagged release with the latest GitHub release by version", ctx do
      configure(ctx, "v0.9.0", request_file: Path.join(ctx.dir, "request"))

      stub_github(%{
        "/repos/cfbender/the-gathering/releases/latest" => %{
          "tag_name" => "v0.10.0",
          "html_url" => "https://github.com/cfbender/the-gathering/releases/tag/v0.10.0"
        }
      })

      assert %{
               version: "v0.9.0",
               channel: :release,
               method: :systemd,
               update_available: true,
               latest: %{
                 version: "v0.10.0",
                 url: "https://github.com/cfbender/the-gathering/releases/tag/v0.10.0"
               }
             } = SelfUpdate.status()
    end

    test "a nightly build follows the nightly tag's commit", ctx do
      configure(ctx, "nightly-0123456", watchtower_token: "secret")

      stub_github(%{
        "/repos/cfbender/the-gathering/git/ref/tags/nightly" => %{
          "object" => %{"sha" => @nightly_sha}
        }
      })

      assert %{
               channel: :nightly,
               method: :watchtower,
               update_available: false,
               latest: %{version: "nightly-0123456"}
             } = SelfUpdate.status()
    end

    test "caches the GitHub answer and reports failures without raising", ctx do
      configure(ctx, "v0.9.0", [])
      parent = self()

      Req.Test.expect(__MODULE__, 1, fn conn ->
        send(parent, :github_called)

        conn
        |> Plug.Conn.put_resp_header("x-ratelimit-remaining", "0")
        |> Plug.Conn.put_status(403)
        |> Req.Test.json(%{"message" => "API rate limit exceeded"})
      end)

      assert %{latest: nil, update_available: nil, check_error: error} = SelfUpdate.status()
      assert error =~ "rate limit"
      assert_receive :github_called
      assert %{check_error: ^error} = SelfUpdate.status()
      refute_receive :github_called
    end
  end

  describe "update_available?/3" do
    test "orders release versions numerically, not lexically" do
      assert SelfUpdate.update_available?(:release, "v0.9.0", "v0.10.0")
      refute SelfUpdate.update_available?(:release, "v0.10.0", "v0.9.0")
      refute SelfUpdate.update_available?(:release, "v1.2.3", "v1.2.3")
    end

    test "accepts nightly commits of different lengths" do
      refute SelfUpdate.update_available?(:nightly, "nightly-0123456", "nightly-0123456789ab")
      assert SelfUpdate.update_available?(:nightly, "nightly-0123456", "nightly-fedcba9")
    end
  end

  describe "request_update/0" do
    test "without an updater it refuses", ctx do
      configure(ctx, "v0.9.0", [])
      assert {:error, :unsupported} = SelfUpdate.request_update()
    end

    test "with systemd it creates the request file and reports the update as pending", ctx do
      request_file = Path.join(ctx.dir, "update-request")
      configure(ctx, "v0.9.0", request_file: request_file)
      stub_github(%{})

      assert {:ok, %{pending: true, requested_at: %DateTime{}}} = SelfUpdate.request_update()
      assert File.exists?(request_file)
      assert %{pending: true} = SelfUpdate.status()

      # systemd removes the file when `update` finishes. Even without the restart a successful
      # update brings, that ends the pending state, so a failed run does not lock the button.
      File.rm!(request_file)
      assert %{pending: false, requested_at: %DateTime{}} = SelfUpdate.status()
    end

    test "with systemd an unwritable request file is reported, not raised", ctx do
      configure(ctx, "v0.9.0", request_file: Path.join(ctx.dir, "missing/dir/update-request"))

      log =
        capture_log(fn -> assert {:error, :updater_unavailable} = SelfUpdate.request_update() end)

      assert log =~ "Could not write the update request file"
    end

    test "with Watchtower it posts an async update for this image only", ctx do
      configure(ctx, "nightly-0123456",
        watchtower_token: "secret",
        watchtower_url: "http://updater:9000"
      )

      parent = self()

      Req.Test.expect(__MODULE__, 1, fn conn ->
        send(
          parent,
          {:watchtower, conn.method, conn.host, conn.port, conn.request_path, conn.query_params,
           Plug.Conn.get_req_header(conn, "authorization")}
        )

        conn |> Plug.Conn.put_status(202) |> Req.Test.json(%{})
      end)

      assert {:ok, %{pending: true}} = SelfUpdate.request_update()

      assert_receive {:watchtower, "POST", "updater", 9000, "/v1/update",
                      %{"image" => "ghcr.io/cfbender/the-gathering", "async" => "true"},
                      ["Bearer secret"]}
    end

    test "Watchtower already updating is distinguished from Watchtower being down", ctx do
      configure(ctx, "v0.9.0", watchtower_token: "secret")

      Req.Test.expect(__MODULE__, 1, fn conn ->
        conn |> Plug.Conn.put_status(429) |> Req.Test.json(%{"error" => "another update"})
      end)

      assert {:error, :update_in_progress} = SelfUpdate.request_update()

      Req.Test.expect(__MODULE__, 1, fn conn -> Req.Test.transport_error(conn, :econnrefused) end)

      log =
        capture_log(fn -> assert {:error, :updater_unavailable} = SelfUpdate.request_update() end)

      assert log =~ "could not be reached"
      assert %{pending: false} = SelfUpdate.status()
    end
  end
end
