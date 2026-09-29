defmodule TheGatheringWeb.CrossOriginIsolation do
  @moduledoc """
  Serves a document cross-origin isolated (COOP `same-origin` + COEP `require-corp`).

  Isolation gives the page `SharedArrayBuffer`, which onnxruntime-web needs to run the card
  recognizer on several WASM threads. Only the webcam table (`/table/*`) is isolated: it loads
  nothing cross-origin, while the rest of the app embeds Discord avatars and other third-party
  images that COEP would block. `require-corp` rather than `credentialless` because Safari does
  not support the latter. See docs/webcam-table.md, "Browser inference in the clicking browser".
  """
  import Plug.Conn

  def init(opts), do: opts

  def call(conn, _opts) do
    conn
    |> put_resp_header("cross-origin-opener-policy", "same-origin")
    |> put_resp_header("cross-origin-embedder-policy", "require-corp")
  end
end
