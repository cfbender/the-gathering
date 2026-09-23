defmodule TheGatheringWeb.API.CardIdCorrectionControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.CardId.Corrections

  setup :register_and_log_in_user

  setup %{user: user} do
    TheGathering.RateLimiter.set({:corrections, user.id}, 60_000, 0)
    previous = Application.fetch_env!(:the_gathering, :data_dir)
    root = Path.join(System.tmp_dir!(), "corrections-#{Ecto.UUID.generate()}")
    Application.put_env(:the_gathering, :data_dir, root)
    export = Application.get_env(:the_gathering, :cardid_corrections_export)

    on_exit(fn ->
      Application.put_env(:the_gathering, :data_dir, previous)
      Application.put_env(:the_gathering, :cardid_corrections_export, export)
      File.rm_rf!(root)
    end)

    %{payload: payload()}
  end

  test "stores native JPEG and label metadata, retries once and allows relabelling", %{
    conn: conn,
    payload: p
  } do
    for _ <- 1..2 do
      assert %{"data" => %{"capture_id" => id}} =
               conn |> post(~p"/api/cardid/corrections", p) |> json_response(201)

      assert id == p["capture_id"]
    end

    assert %{cursor: 1, corrections: [row]} = Corrections.page(0)
    assert row["label"] == p["label"]
    assert row["click"] == [123, 456]
    assert row["quad"] == p["quad"]
    assert row["up_vote"] == 0.92
    assert row["split"] in ["train", "eval"]
    refute Map.has_key?(row, "image")
    refute Map.has_key?(row, "up_correct")
    assert {:ok, path} = Corrections.crop_path(p["capture_id"])
    assert File.read!(path) == File.read!("test/support/fixtures/cardid-crop.jpg")

    relabelled = Map.put(p, "label", Ecto.UUID.generate())
    assert conn |> post(~p"/api/cardid/corrections", relabelled) |> json_response(201)
    assert %{cursor: 2, corrections: [next]} = Corrections.page(1)
    assert next["label"] == relabelled["label"]
    assert next["split"] == row["split"]
  end

  test "preserves face labels and top-1 through storage and export", %{conn: conn, payload: p} do
    face = p["label"] <> "-1"
    payload = Map.merge(p, %{"label" => face, "top1" => face})
    assert conn |> post(~p"/api/cardid/corrections", payload) |> json_response(201)
    admin = log_in_user(build_conn(), AccountsFixtures.admin_fixture())
    body = admin |> get(~p"/api/cardid/corrections") |> json_response(200)
    assert [row] = body["data"]["corrections"]
    assert row["label"] == face
    assert row["top1"] == face
    assert row["capture_id"] == p["capture_id"]
  end

  test "stores exact sibling and Revised printing labels independently of the ranked art", %{
    conn: conn,
    payload: p
  } do
    for label <- [
          "a51fb64d-cc0c-400d-971f-78c28d42043b",
          "97fa5f07-46ba-408d-a861-bdb1791cc188",
          "cb9b9a9d-ae4c-4e04-bf9d-cae48f01292c",
          "6d6deae3-3ed4-47eb-bf4a-4a766ce18135"
        ] do
      payload = Map.merge(p, %{"label" => label, "capture_id" => Ecto.UUID.generate()})
      assert conn |> post(~p"/api/cardid/corrections", payload) |> json_response(201)
    end

    assert %{corrections: rows} = Corrections.page(0)

    assert Enum.map(rows, & &1["label"]) == [
             "a51fb64d-cc0c-400d-971f-78c28d42043b",
             "97fa5f07-46ba-408d-a861-bdb1791cc188",
             "cb9b9a9d-ae4c-4e04-bf9d-cae48f01292c",
             "6d6deae3-3ed4-47eb-bf4a-4a766ce18135"
           ]

    assert Enum.all?(rows, &(&1["top1"] == p["top1"]))
  end

  test "rejects malformed, oversized, and traversing payloads", %{conn: conn, payload: p} do
    for change <- [
          %{"capture_id" => "../escape"},
          %{"capture_id" => p["capture_id"] <> "-1"},
          %{"label" => "not-a-scryfall-id"},
          %{"label" => p["label"] <> "-0"},
          %{"label" => p["label"] <> "-2"},
          %{"label" => p["label"] <> "-01"},
          %{"top1" => p["label"] <> "-1/../escape"},
          %{"image" => "data:image/png;base64,AAAA"},
          %{"image" => "data:image/jpeg;base64," <> String.duplicate("A", 190_004)},
          %{"image" => "data:image/jpeg;base64,AAAA"},
          %{"image" => "data:image/jpeg;base64," <> Base.encode64(<<255, 216, 255, 217>>)},
          %{"quad" => [[1, 2]]},
          %{"click" => [641, 10]},
          %{"similarity" => "0.5"},
          %{"bundle_version" => String.duplicate("a", 121)}
        ] do
      assert conn |> post(~p"/api/cardid/corrections", Map.merge(p, change)) |> json_response(400)
    end

    assert %{cursor: 0} = Corrections.page(0)
  end

  test "anonymous users cannot upload and another user cannot overwrite a capture", %{
    conn: conn,
    payload: p
  } do
    assert build_conn() |> post(~p"/api/cardid/corrections", p) |> json_response(401)
    assert conn |> post(~p"/api/cardid/corrections", p) |> json_response(201)
    other = log_in_user(build_conn(), AccountsFixtures.user_fixture())
    assert other |> post(~p"/api/cardid/corrections", p) |> json_response(403)
  end

  test "rate limit is per user, not address", %{conn: conn, payload: p} do
    config = Application.fetch_env!(:the_gathering, TheGatheringWeb.RateLimit)

    Application.put_env(
      :the_gathering,
      TheGatheringWeb.RateLimit,
      Keyword.put(config, :corrections, limit: 1, scale: 60_000)
    )

    on_exit(fn -> Application.put_env(:the_gathering, TheGatheringWeb.RateLimit, config) end)
    assert conn |> post(~p"/api/cardid/corrections", p) |> json_response(201)
    denied = post(%{conn | remote_ip: {1, 2, 3, 4}}, ~p"/api/cardid/corrections", p)
    assert json_response(denied, 429)
    assert get_resp_header(denied, "retry-after") != []
    user = AccountsFixtures.user_fixture()
    TheGathering.RateLimiter.set({:corrections, user.id}, 60_000, 0)
    other = log_in_user(build_conn(), user)
    assert other |> post(~p"/api/cardid/corrections", payload()) |> json_response(201)
  end

  test "exports require admin and cursor pages do not expose owner IDs", %{conn: conn, payload: p} do
    assert conn |> post(~p"/api/cardid/corrections", p) |> json_response(201)
    assert conn |> get(~p"/api/cardid/corrections") |> json_response(403)
    assert conn |> get(~p"/api/cardid/corrections/#{p["capture_id"]}/crop") |> json_response(403)
    admin = log_in_user(build_conn(), AccountsFixtures.admin_fixture())

    assert %{"data" => %{"cursor" => 1, "corrections" => [row], "has_more" => false}} =
             admin |> get(~p"/api/cardid/corrections") |> json_response(200)

    refute Map.has_key?(row, "user_id")

    assert %{"data" => %{"corrections" => []}} =
             admin |> get(~p"/api/cardid/corrections?cursor=1") |> json_response(200)

    assert admin |> get(~p"/api/cardid/corrections?cursor=-1") |> json_response(400)
    assert admin |> get(~p"/api/cardid/corrections?cursor[]=1") |> json_response(400)
    assert admin |> get(~p"/api/cardid/corrections/..%2Fescape/crop") |> response(404)
    assert admin |> get(~p"/api/cardid/corrections/#{p["capture_id"]}/crop") |> response(200)
  end

  test "scoped bearer token is read-only and revoked when its administrator is disabled", %{
    payload: p
  } do
    admin = AccountsFixtures.admin_fixture()
    token = String.duplicate("t", 40)

    Application.put_env(:the_gathering, :cardid_corrections_export,
      token: token,
      admin_id: admin.id
    )

    conn = put_req_header(build_conn(), "authorization", "Bearer " <> token)
    assert conn |> get(~p"/api/cardid/corrections") |> json_response(200)
    assert conn |> post(~p"/api/cardid/corrections", p) |> json_response(401)

    assert build_conn()
           |> put_req_header("authorization", "Bearer wrong")
           |> get(~p"/api/cardid/corrections")
           |> json_response(403)

    admin
    |> Ecto.Changeset.change(disabled_at: DateTime.utc_now(:second))
    |> TheGathering.Repo.update!()

    assert conn |> get(~p"/api/cardid/corrections") |> json_response(403)
  end

  defp payload do
    %{
      "capture_id" => Ecto.UUID.generate(),
      "label" => Ecto.UUID.generate(),
      "image" =>
        "data:image/jpeg;base64," <>
          Base.encode64(File.read!("test/support/fixtures/cardid-crop.jpg")),
      "click" => [123, 456],
      "quad" => [[40, 20], [290, 20], [290, 370], [40, 370]],
      "up_vote" => 0.92,
      "bundle_version" => "full-3",
      "top1" => Ecto.UUID.generate(),
      "similarity" => 0.7,
      "margin" => 0.02
    }
  end
end
