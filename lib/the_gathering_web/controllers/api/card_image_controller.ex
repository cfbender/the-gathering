defmodule TheGatheringWeb.API.CardImageController do
  use TheGatheringWeb, :controller

  alias TheGathering.Catalog.CardImages

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, %{"url" => url}) do
    with {:ok, body, cache} <- CardImages.fetch(url) do
      etag = ~s("#{Base.encode16(:crypto.hash(:sha256, body), case: :lower)}")

      conn =
        conn
        |> put_resp_content_type("image/jpeg")
        |> put_resp_header("cache-control", "private, max-age=86400")
        |> put_resp_header("etag", etag)
        |> put_resp_header("x-card-image-cache", cache)
        |> put_resp_header("x-content-type-options", "nosniff")

      if etag in get_req_header(conn, "if-none-match"),
        do: send_resp(conn, 304, ""),
        else: send_resp(conn, 200, body)
    end
  end

  def show(_conn, _params), do: {:error, :bad_request}
end
