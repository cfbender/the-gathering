defmodule TheGatheringWeb.ViteAssets do
  @moduledoc """
  Resolves the `<script>` and `<link>` tags for the React entrypoint.

  In development the tags point at the Vite dev server (HMR, React Fast
  Refresh). In production they are read from the Vite build manifest under
  `priv/static/assets/react/.vite/manifest.json`, so hashed filenames stay
  cacheable forever.
  """

  @entry "assets/react/src/main.tsx"
  @manifest_path "static/assets/react/.vite/manifest.json"
  @public_path "/assets/react/"
  @proxy_header "x-the-gathering-vite-proxy"

  @doc "Returns HTML for the React entrypoint appropriate to the current mode."
  @spec tags(Plug.Conn.t()) :: String.t()
  def tags(conn) do
    case mode() do
      :dev_server -> dev_server_tags(conn)
      :manifest -> manifest_tags()
    end
  end

  defp mode do
    :the_gathering
    |> Application.get_env(__MODULE__, [])
    |> Keyword.get(:mode, :manifest)
  end

  defp dev_server_tags(conn) do
    # Requests proxied through the Vite dev server can use relative URLs so
    # the page also works when served through a tunnel on the Vite port.
    origin =
      case Plug.Conn.get_req_header(conn, @proxy_header) do
        [] -> dev_server_origin()
        _proxied -> ""
      end

    """
    <script type="module">
      import RefreshRuntime from "#{origin}/@react-refresh"
      RefreshRuntime.injectIntoGlobalHook(window)
      window.$RefreshReg$ = () => {}
      window.$RefreshSig$ = () => (type) => type
      window.__vite_plugin_react_preamble_installed__ = true
    </script>
    <script type="module" src="#{origin}/@vite/client"></script>
    <script type="module" src="#{origin}/#{@entry}"></script>
    """
  end

  defp dev_server_origin do
    :the_gathering
    |> Application.get_env(__MODULE__, [])
    |> Keyword.get(:dev_server_origin, "http://127.0.0.1:5173")
  end

  defp manifest_tags do
    %{"file" => file} = entry = manifest() |> Map.fetch!(@entry)

    styles =
      entry
      |> Map.get("css", [])
      |> Enum.map_join("\n", &~s(<link rel="stylesheet" href="#{@public_path}#{&1}" />))

    styles <> "\n" <> ~s(<script type="module" src="#{@public_path}#{file}"></script>)
  end

  defp manifest do
    case :persistent_term.get({__MODULE__, :manifest}, nil) do
      nil ->
        manifest =
          :the_gathering
          |> :code.priv_dir()
          |> Path.join(@manifest_path)
          |> File.read!()
          |> Jason.decode!()

        :persistent_term.put({__MODULE__, :manifest}, manifest)
        manifest

      manifest ->
        manifest
    end
  end
end
