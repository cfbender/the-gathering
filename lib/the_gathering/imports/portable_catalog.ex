defmodule TheGathering.Imports.PortableCatalog do
  @moduledoc false
  import Ecto.Changeset

  alias TheGathering.Catalog.{Card, Printing}
  alias TheGathering.Imports.PortableFile
  alias TheGathering.Repo

  def restore(data) do
    Enum.each(data["cards"], &card/1)
    Enum.each(data["printings"], &printing/1)
  end

  defp card(attrs) do
    changeset =
      %Card{}
      |> cast(PortableFile.attrs(attrs, :cards), PortableFile.fields(:cards))
      |> validate_required(
        ~w(id oracle_id name normalized_name cmc type_line image_uris set_code collector_number layout rarity commander_legal can_be_commander)a
      )
      |> validate_arrays()
      |> unique_constraint(:oracle_id)

    validate!(changeset)
    id = get_field(changeset, :id)
    oracle_id = get_field(changeset, :oracle_id)
    unless Repo.get(Card, id) || Repo.get_by(Card, oracle_id: oracle_id), do: insert!(changeset)
  end

  defp printing(attrs) do
    changeset =
      %Printing{}
      |> cast(PortableFile.attrs(attrs, :printings), PortableFile.fields(:printings))
      |> validate_required(PortableFile.fields(:printings))

    validate!(changeset)
    unless Repo.get(Printing, get_field(changeset, :id)), do: insert!(changeset)
  end

  defp validate_arrays(changeset) do
    Enum.reduce([:colors, :color_identity], changeset, fn field, changeset ->
      if is_list(get_field(changeset, field)),
        do: changeset,
        else: add_error(changeset, field, "must be a list")
    end)
  end

  defp validate!(%{valid?: true}), do: :ok
  defp validate!(_changeset), do: Repo.rollback("Invalid card-art data in export.")

  defp insert!(changeset) do
    case Repo.insert(changeset) do
      {:ok, _} -> :ok
      {:error, _} -> Repo.rollback("Could not restore card-art data.")
    end
  end
end
