defmodule TheGathering.Discord.SummaryUploadTest do
  use ExUnit.Case, async: true

  alias Nostrum.Error.ApiError
  alias TheGathering.Discord.SummaryUpload

  @interaction %{application_id: 123, token: "test-token"}

  test "patches the original response with matching PNG and attachment metadata" do
    png = <<137, 80, 78, 71, 13, 10, 26, 10, 0, 255>>

    response = %{
      content: "Game #219",
      allowed_mentions: %{parse: []},
      attachments: [%{id: 0, filename: "game-219.png", description: "Winner: Alice"}],
      files: [%{name: "game-219.png", body: png}]
    }

    Req.Test.expect(__MODULE__, fn conn ->
      assert conn.method == "PATCH"
      assert conn.host == "discord.com"
      assert conn.request_path == "/api/v10/webhooks/123/test-token/messages/@original"
      assert Plug.Conn.get_req_header(conn, "authorization") == []

      assert %{"payload_json" => json, "files" => %{"0" => upload}} = conn.body_params
      assert map_size(conn.body_params) == 2

      assert Jason.decode!(json) == %{
               "content" => "Game #219",
               "allowed_mentions" => %{"parse" => []},
               "attachments" => [
                 %{"id" => 0, "filename" => "game-219.png", "description" => "Winner: Alice"}
               ]
             }

      assert upload.content_type == "image/png"
      assert upload.filename == "game-219.png"
      assert File.read!(upload.path) == png
      Req.Test.json(conn, %{id: "456"})
    end)

    assert {:ok, %{"id" => "456"}} =
             SummaryUpload.edit_response(@interaction, response, plug: {Req.Test, __MODULE__})
  end

  test "rendering errors can still replace the deferred response with plain text" do
    Req.Test.expect(__MODULE__, fn conn ->
      assert Plug.Conn.get_req_header(conn, "content-type") == ["application/json"]
      {:ok, body, conn} = Plug.Conn.read_body(conn)

      assert Jason.decode!(body) == %{
               "content" => "Render failed",
               "allowed_mentions" => %{"parse" => []}
             }

      Req.Test.json(conn, %{id: "456"})
    end)

    assert {:ok, _} =
             SummaryUpload.edit_response(
               @interaction,
               %{content: "Render failed", allowed_mentions: %{parse: []}},
               plug: {Req.Test, __MODULE__}
             )
  end

  test "timeouts propagate without retries and all network waits have finite limits" do
    assert {:error, %Req.TransportError{reason: :timeout}} =
             SummaryUpload.edit_response(@interaction, %{content: "test"}, adapter: __MODULE__)

    assert_receive :upload_attempt
    refute_receive :upload_attempt
  end

  test "Discord errors and redirects return once rather than retrying or forwarding the token" do
    for status <- [302, 403, 429, 500] do
      Req.Test.expect(__MODULE__, fn conn ->
        conn
        |> Plug.Conn.put_resp_header("location", "https://example.com/should-not-follow")
        |> Plug.Conn.put_status(status)
        |> Req.Test.json(%{code: 50_013, retry_after: 0.01})
      end)

      assert {:error, %ApiError{status_code: ^status, response: %{"code" => 50_013}}} =
               SummaryUpload.edit_response(@interaction, %{content: "test"},
                 plug: {Req.Test, __MODULE__}
               )
    end
  end

  def run(request) do
    assert request.options.connect_options[:timeout] == 3_000
    assert request.options.finch[:pool_timeout] == 3_000
    assert request.options.receive_timeout == 15_000
    assert request.options.request_timeout == 15_000
    assert request.options.retry == false
    assert request.options.redirect == false
    send(self(), :upload_attempt)

    {request, %Req.TransportError{reason: :timeout}}
  end
end
