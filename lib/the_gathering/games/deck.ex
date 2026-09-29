defmodule TheGathering.Games.Deck do
  use Ecto.Schema
  import Ecto.Changeset

  alias TheGathering.Catalog
  alias TheGathering.Games.{ColorIdentity, DeckPrintings}

  @sources ~w(moxfield archidekt manavault other)
  @identity_fields ~w(color_identity commander_card_id commander_name partner_card_id partner_name)a

  schema "decks" do
    field :name, :string
    field :commander_card_id, :string
    field :commander_name, :string
    field :commander_printing_id, :string
    field :partner_card_id, :string
    field :partner_name, :string
    field :partner_printing_id, :string
    field :color_identity, :string, default: ""
    field :decklist_url, :string
    field :decklist_source, :string
    field :archived_at, :utc_datetime
    field :skip_count, :integer, default: 0
    field :included_for_play, :boolean, default: true

    belongs_to :player, TheGathering.Games.Player
    has_many :game_players, TheGathering.Games.GamePlayer

    timestamps(type: :utc_datetime)
  end

  def changeset(deck, attrs) do
    deck
    |> cast(attrs, [
      :player_id,
      :name,
      :commander_card_id,
      :commander_name,
      :commander_printing_id,
      :partner_card_id,
      :partner_name,
      :partner_printing_id,
      :color_identity,
      :decklist_url,
      :archived_at,
      :included_for_play
    ])
    |> update_change(:name, &String.trim/1)
    |> update_change(:commander_name, &String.trim/1)
    |> put_decklist_source()
    |> DeckPrintings.validate()
    |> validate_required([:player_id, :name, :commander_name])
    |> validate_length(:name, min: 1, max: 100)
    |> validate_number(:skip_count, greater_than_or_equal_to: 0)
    |> validate_color_identity()
    |> validate_inclusion(:decklist_source, @sources)
    |> assoc_constraint(:player)
    # ecto_sqlite3 cannot learn which index fired and reports the violated columns
    # as `decks_player_id_name_index`, so declare that name as well.
    |> unique_constraint(:name, name: :decks_player_name_nocase_index)
    |> unique_constraint(:name, name: :decks_player_id_name_index)
    |> unique_constraint(:name)
  end

  def update_changeset(deck, attrs) do
    deck
    |> cast(attrs, [
      :name,
      :commander_card_id,
      :commander_name,
      :commander_printing_id,
      :partner_card_id,
      :partner_name,
      :partner_printing_id,
      :color_identity,
      :decklist_url,
      :archived_at,
      :included_for_play
    ])
    |> update_change(:name, &String.trim/1)
    |> update_change(:commander_name, &String.trim/1)
    |> put_decklist_source()
    |> DeckPrintings.validate()
    |> validate_required([:player_id, :name, :commander_name])
    |> validate_length(:name, min: 1, max: 100)
    |> validate_color_identity()
    |> validate_inclusion(:decklist_source, @sources)
    |> unique_constraint(:name, name: :decks_player_name_nocase_index)
    |> unique_constraint(:name, name: :decks_player_id_name_index)
    |> unique_constraint(:name)
  end

  defp put_decklist_source(changeset) do
    case get_field(changeset, :decklist_url) do
      url when is_binary(url) and url != "" ->
        put_change(changeset, :decklist_source, source(url))

      _url ->
        put_change(changeset, :decklist_source, nil)
    end
  end

  defp source(url) do
    host = URI.parse(url).host || ""

    cond do
      host == "moxfield.com" or String.ends_with?(host, ".moxfield.com") -> "moxfield"
      host == "archidekt.com" or String.ends_with?(host, ".archidekt.com") -> "archidekt"
      host == "manavault.app" or String.ends_with?(host, ".manavault.app") -> "manavault"
      true -> "other"
    end
  end

  defp validate_color_identity(changeset) do
    value = get_field(changeset, :color_identity) || ""

    if Regex.match?(~r/^(?!.*(.).*\1)[WUBRG]*$/, value) do
      include_commander_colors(changeset, value)
    else
      add_error(changeset, :color_identity, "must contain each of W, U, B, R, and G at most once")
    end
  end

  # A deck's identity always covers every commander card's identity, so a partner
  # can never be dropped by whichever client or import wrote the deck. Colors beyond
  # that are kept: they are how owners record chosen colors (Clara Oswald).
  defp include_commander_colors(changeset, value) do
    if Enum.any?(@identity_fields, &changed?(changeset, &1)) do
      put_change(
        changeset,
        :color_identity,
        ColorIdentity.canonical(value <> commander_colors(changeset))
      )
    else
      changeset
    end
  end

  defp commander_colors(changeset) do
    refs =
      [
        {get_field(changeset, :commander_card_id), get_field(changeset, :commander_name)},
        {get_field(changeset, :partner_card_id), get_field(changeset, :partner_name)}
      ]
      |> Enum.reject(&(&1 == {nil, nil}))

    summaries = Catalog.card_summaries(refs)

    refs
    |> Enum.map(fn {id, name} -> Catalog.card_summary(summaries, id, name) end)
    |> Enum.reject(&is_nil/1)
    |> Enum.map_join(& &1.color_identity)
  end
end
