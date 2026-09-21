defmodule TheGathering.Games.DeckPrintings do
  @moduledoc false

  import Ecto.Changeset

  alias TheGathering.Catalog

  def validate(changeset) do
    changeset
    |> validate_slot(:commander_card_id, :commander_name, :commander_printing_id)
    |> validate_slot(:partner_card_id, :partner_name, :partner_printing_id)
  end

  defp validate_slot(changeset, id_field, name_field, printing_field) do
    identity_changed? = changed?(changeset, id_field) or changed?(changeset, name_field)
    supplied? = Map.has_key?(changeset.params || %{}, Atom.to_string(printing_field))

    cond do
      identity_changed? and not supplied? ->
        put_change(changeset, printing_field, nil)

      identity_changed? or changed?(changeset, printing_field) ->
        validate_printing(changeset, id_field, name_field, printing_field)

      true ->
        changeset
    end
  end

  defp validate_printing(changeset, id_field, name_field, printing_field) do
    case get_field(changeset, printing_field) do
      nil ->
        changeset

      printing_id ->
        id = get_field(changeset, id_field)

        with name when is_binary(name) <- get_field(changeset, name_field),
             %{oracle_id: oracle_id} <- Catalog.resolve_card(id, name),
             %{oracle_id: ^oracle_id} <- Catalog.find_card_by_name(name),
             %{oracle_id: ^oracle_id} <- Catalog.get_printing(printing_id) do
          foreign_key_constraint(changeset, printing_field)
        else
          _invalid ->
            add_error(changeset, printing_field, "must be a printing of the selected card")
        end
    end
  end
end
