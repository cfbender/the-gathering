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
        #{manavault_meta()}
        <meta name="theme-color" content="#f5e6e2" media="(prefers-color-scheme: light)" />
        <meta name="theme-color" content="#180810" media="(prefers-color-scheme: dark)" />
        <title>The Gathering</title>
        <link rel="icon" href="/favicon.ico" sizes="32x32" />
        <link rel="icon" href="/images/logo.svg" type="image/svg+xml" />
        <link rel="apple-touch-icon" href="/images/apple-touch-icon.png" />
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

  # Lets the SPA label links to the configured self-hosted ManaVault (see `lib/decklists.ts`).
  defp manavault_meta do
    case TheGathering.Decklists.manavault_url() do
      nil -> ""
      uri -> ~s(<meta name="manavault-url" content="#{Plug.HTML.html_escape(to_string(uri))}" />)
    end
  end
end
