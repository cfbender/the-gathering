defmodule TheGatheringWeb.AppController do
  use TheGatheringWeb, :controller

  alias TheGatheringWeb.ViteAssets

  @doc "Serves the single-page React application shell."
  def index(conn, _params) do
    conn
    |> put_resp_content_type("text/html")
    |> put_resp_header("cache-control", "no-cache, no-store, must-revalidate")
    |> send_resp(200, shell_html(conn))
  end

  defp shell_html(conn) do
    """
    <!DOCTYPE html>
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="csrf-token" content="#{get_csrf_token()}" />
        <meta name="application-name" content="The Gathering" />
        <meta name="theme-color" content="#1c1917" />
        <title>The Gathering</title>
        <link rel="icon" href="/favicon.ico" sizes="any" />
        <script>
          (() => {
            const key = "the-gathering:theme"
            let stored = null
            try { stored = localStorage.getItem(key) } catch {}
            const system = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"
            document.documentElement.dataset.theme = stored === "light" || stored === "dark" ? stored : system
          })()
        </script>
        #{ViteAssets.tags(conn)}
      </head>
      <body>
        <div id="root"></div>
      </body>
    </html>
    """
  end
end
