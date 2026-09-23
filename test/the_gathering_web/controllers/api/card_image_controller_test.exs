defmodule TheGatheringWeb.API.CardImageControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Catalog.CardImages

  @source "https://cards.scryfall.io/normal/back/a/b/abcdef01-2345-6789-abcd-ef0123456789.jpg?123"
  @jpeg <<255, 216, 255, 224, 1, 2, 3>>

  setup :register_and_log_in_user
  setup :set_req_test_to_shared
  setup :verify_on_exit!

  import Req.Test, only: [set_req_test_to_shared: 1, verify_on_exit!: 1]

  setup do
    original = Application.fetch_env!(:the_gathering, :data_dir)
    root = Path.join(System.tmp_dir!(), "card-images-#{System.unique_integer([:positive])}")
    :ok = Supervisor.terminate_child(TheGathering.Catalog.Supervisor, CardImages)
    Application.put_env(:the_gathering, :data_dir, root)
    Application.put_env(:the_gathering, :card_image_req_options, plug: {Req.Test, __MODULE__})
    start_supervised!(CardImages)
    tasks = start_supervised!(Task.Supervisor)

    on_exit(fn ->
      Application.put_env(:the_gathering, :data_dir, original)
      Application.delete_env(:the_gathering, :card_image_req_options)
      Supervisor.restart_child(TheGathering.Catalog.Supervisor, CardImages)
      File.rm_rf!(root)
    end)

    %{tasks: tasks, root: root}
  end

  test "serves unchanged bytes for image Accept headers, caches across views, and revalidates", %{
    conn: conn,
    root: root
  } do
    Req.Test.expect(__MODULE__, fn request ->
      assert request.host == "cards.scryfall.io"
      assert request.query_string == "123"
      assert get_req_header(request, "accept") == ["image/jpeg"]
      send_resp(request, 200, @jpeg)
    end)

    url = CardImages.url(@source)
    conn = put_req_header(conn, "accept", "image/avif,image/webp,image/*,*/*;q=0.8")
    first = get(conn, url)
    assert response(first, 200) == @jpeg
    assert get_resp_header(first, "x-card-image-cache") == ["miss"]
    assert get_resp_header(first, "cache-control") == ["private, max-age=86400"]
    assert [_file] = Path.wildcard(Path.join(root, "card-images/*.jpg"))
    second = get(conn, url)
    assert response(second, 200) == @jpeg
    assert get_resp_header(second, "x-card-image-cache") == ["hit"]
    [etag] = get_resp_header(second, "etag")
    assert conn |> put_req_header("if-none-match", etag) |> get(url) |> response(304) == ""
  end

  test "rejects arbitrary origins, credentials, ports, traversal, and redirects", %{conn: conn} do
    for source <- [
          "http://cards.scryfall.io/normal/a.jpg",
          "https://localhost/a.jpg",
          String.replace(@source, "cards.scryfall.io", "cards.scryfall.io.evil.test"),
          String.replace(@source, "cards.scryfall.io", "user@cards.scryfall.io"),
          String.replace(@source, "cards.scryfall.io", "cards.scryfall.io:443"),
          String.replace(@source, "/normal/", "/normal/../"),
          @source <> "&extra=1"
        ] do
      assert conn |> get("/api/card-images", url: source) |> json_response(400)
    end

    Req.Test.expect(__MODULE__, fn request ->
      request |> put_resp_header("location", "http://localhost/private") |> send_resp(302, "")
    end)

    assert conn |> get(CardImages.url(@source)) |> json_response(502)
  end

  test "deduplicates in-flight fetches and bounds distinct upstream concurrency", %{tasks: tasks} do
    owner = self()

    Req.Test.expect(__MODULE__, 5, fn request ->
      send(owner, {:fetching, self(), request.query_string})
      receive do: (:release -> send_resp(request, 200, @jpeg))
    end)

    first = Task.Supervisor.async_nolink(tasks, fn -> CardImages.fetch(@source) end)
    assert_receive {:fetching, pid, "123"}
    duplicate = Task.Supervisor.async_nolink(tasks, fn -> CardImages.fetch(@source) end)

    rest =
      for i <- 1..4,
          do:
            Task.Supervisor.async_nolink(tasks, fn ->
              CardImages.fetch(@source <> to_string(i))
            end)

    pids =
      for _ <- 1..3 do
        assert_receive {:fetching, other, _query}
        other
      end

    state = :sys.get_state(CardImages)
    assert map_size(state.tasks) == 4
    refute_receive {:fetching, _, _}, 20
    send(pid, :release)
    assert_receive {:fetching, fifth, _query}
    Enum.each([fifth | pids], &send(&1, :release))
    assert {:ok, @jpeg, "miss"} = Task.await(first)
    assert {:ok, @jpeg, _hit_or_miss} = Task.await(duplicate)
    Enum.each(rest, fn task -> assert {:ok, @jpeg, "miss"} = Task.await(task) end)
  end

  test "does not cache failures or non-images", %{conn: conn, root: root} do
    Req.Test.expect(__MODULE__, fn request -> send_resp(request, 200, "not a JPEG") end)
    assert conn |> get(CardImages.url(@source)) |> json_response(502)
    assert Path.wildcard(Path.join(root, "card-images/*.jpg")) == []
    Req.Test.expect(__MODULE__, fn request -> send_resp(request, 200, @jpeg) end)
    assert conn |> get(CardImages.url(@source)) |> response(200) == @jpeg
  end

  test "honors CDN rate limiting across different image URLs", %{conn: conn} do
    Req.Test.expect(__MODULE__, fn request ->
      request |> put_resp_header("retry-after", "90") |> send_resp(429, "slow down")
    end)

    assert conn |> get(CardImages.url(@source)) |> json_response(502)
    assert conn |> get(CardImages.url(@source <> "1")) |> json_response(502)
    assert :sys.get_state(CardImages).paused_until >= System.os_time(:second) + 89
  end

  test "prunes expired and oldest files at startup without losing fresh cached data", %{
    root: root
  } do
    stop_supervised!(CardImages)
    directory = Path.join(root, "card-images")
    old = Path.join(directory, "old.jpg")
    expired = Path.join(directory, "expired.jpg")
    fresh = Path.join(directory, "fresh.jpg")
    File.write!(old, String.duplicate("x", 20))
    File.touch!(old, System.os_time(:second) - 10)
    File.write!(expired, @jpeg)
    File.touch!(expired, System.os_time(:second) - 30 * 86_400)
    # A sparse file tests the real 512 MiB limit without allocating that much RAM or disk.
    {:ok, file} = :file.open(String.to_charlist(fresh), [:write, :binary])
    {:ok, _} = :file.position(file, 512 * 1024 * 1024 - 11)
    :ok = :file.write(file, <<0>>)
    :ok = :file.close(file)
    start_supervised!(CardImages)
    refute File.exists?(expired)
    refute File.exists?(old)
    assert File.exists?(fresh)
  end

  test "aborts an oversized response instead of storing it", %{conn: conn, root: root} do
    Req.Test.expect(__MODULE__, fn request ->
      send_resp(request, 200, @jpeg <> :binary.copy(<<0>>, 2 * 1024 * 1024))
    end)

    assert conn |> get(CardImages.url(@source)) |> json_response(502)
    assert Path.wildcard(Path.join(root, "card-images/*.jpg")) == []
  end

  test "requires a session before fetching", %{conn: conn} do
    previous = Application.get_env(:the_gathering, :dev_auto_login, false)
    Application.put_env(:the_gathering, :dev_auto_login, false)
    on_exit(fn -> Application.put_env(:the_gathering, :dev_auto_login, previous) end)

    assert conn
           |> recycle()
           |> init_test_session(%{})
           |> get(CardImages.url(@source))
           |> json_response(401)
  end
end
