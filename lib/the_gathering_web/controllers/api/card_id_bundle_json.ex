defmodule TheGatheringWeb.API.CardIdBundleJSON do
  @doc "The current bundle: its manifest (version, gallery, constants) plus file URLs."
  def show(%{manifest: manifest, files: files}) do
    %{
      data: %{
        version: manifest["version"],
        created: manifest["created"],
        gallery: manifest["gallery"],
        constants: manifest["constants"],
        files: files
      }
    }
  end
end
