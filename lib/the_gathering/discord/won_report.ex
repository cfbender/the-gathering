defmodule TheGathering.Discord.WonReport do
  @moduledoc "Participant-scoped result drafts; only confirmation consumes a pending game."

  import Ecto.Query, only: [from: 2]

  alias TheGathering.{Accounts, Discord, Repo}
  alias TheGathering.Discord.{PendingGame, ResultDetails, ResultDraft, Sink}
  alias TheGathering.Discord.Sink.Games, as: GamesSink
  alias TheGathering.Games.{Game, WinCondition}

  def open(reference, actor) do
    pending =
      if reference == "" do
        Discord.latest_pending_in_channel(actor.channel_id)
      else
        id = reference |> String.upcase() |> String.trim_leading("#") |> String.trim_leading("SB")
        Discord.get_pending_by_external_id("spellbot:SB#{id}")
      end

    with :ok <- authorize(pending, actor) do
      now = DateTime.utc_now() |> DateTime.truncate(:second)
      Repo.delete_all(from d in ResultDraft, where: d.expires_at < ^now)

      draft =
        Repo.insert!(%ResultDraft{
          pending_game_id: pending.id,
          discord_id: actor.discord_id,
          guild_id: actor.guild_id,
          channel_id: actor.channel_id,
          snapshot: snapshot(pending),
          expires_at: DateTime.add(now, 3600),
          data: %{
            "winner" => actor.discord_id,
            "win_condition" => "unknown",
            "duration" => to_string(max(div(DateTime.diff(now, pending.played_at), 60), 1))
          }
        })

      {:ok, draft, pending}
    end
  end

  def load(id, actor) do
    with {:ok, id} <- Ecto.UUID.cast(id),
         %ResultDraft{} = draft <- Repo.get(ResultDraft, id),
         true <-
           draft.discord_id == actor.discord_id and draft.guild_id == actor.guild_id and
             draft.channel_id == actor.channel_id,
         :gt <- DateTime.compare(draft.expires_at, DateTime.utc_now()),
         %PendingGame{} = pending <- Repo.get(PendingGame, draft.pending_game_id),
         true <- draft.snapshot == snapshot(pending),
         :ok <- authorize(pending, actor) do
      {:ok, draft, pending}
    else
      _ -> {:error, "This draft expired, changed, or is not yours. Run /won again."}
    end
  end

  def act(id, action, values, actor) do
    result =
      Repo.transaction(fn ->
        case load(id, actor) do
          {:ok, draft, pending} -> apply_action(draft, pending, action, values)
          {:error, message} -> Repo.rollback(message)
        end
      end)

    case result do
      {:ok, result} -> result
      {:error, message} -> {:error, message}
    end
  end

  def players(pending), do: Discord.pending_report(pending).players

  def kills_page(pending, page),
    do: pending |> players() |> Enum.chunk_every(5) |> Enum.at(page, [])

  defp apply_action(draft, pending, "details", fields) do
    data = Map.merge(draft.data, Map.take(fields, ~w(turns duration mvp notes)))
    data = data |> Map.put("details_done", true) |> ResultDetails.with_mvp()
    update(draft, pending, data)
  end

  defp apply_action(draft, pending, "kills" <> page, fields) when page in ["0", "1"] do
    keys = Enum.map(kills_page(pending, String.to_integer(page)), &"kills_#{&1.discord_id}")
    update(draft, pending, Map.merge(draft.data, Map.take(fields, keys)))
  end

  defp apply_action(draft, pending, "winner", %{"value" => value}) do
    if value in Enum.map(players(pending), & &1.discord_id),
      do: update(draft, pending, Map.put(draft.data, "winner", value)),
      else: Repo.rollback("Select a winner from this game's players.")
  end

  defp apply_action(draft, pending, "condition", %{"value" => value}) do
    if value in (WinCondition.keys() -- ["draw"]),
      do: update(draft, pending, Map.put(draft.data, "win_condition", value)),
      else: Repo.rollback("Select a valid win condition.")
  end

  defp apply_action(draft, pending, "mvp", %{"value" => value}) do
    case Enum.find(draft.data["mvp_candidates"] || [], &(&1["id"] == value)) do
      nil ->
        Repo.rollback("Select one of the matching MVP cards.")

      card ->
        data =
          Map.merge(draft.data, %{"mvp" => card["name"], "mvp_id" => value, "mvp_error" => nil})

        update(draft, pending, data)
    end
  end

  defp apply_action(draft, pending, "save", _values) do
    case ResultDetails.validate(draft.data, players(pending)) do
      {:ok, details} -> save(draft, pending, details)
      {:error, error} -> {:invalid, draft, pending, error}
    end
  end

  defp apply_action(draft, _pending, "cancel", _values) do
    Repo.delete!(draft)
    :cancelled
  end

  defp apply_action(_draft, _pending, _action, _values),
    do: Repo.rollback("Invalid result action.")

  defp save(draft, pending, details) do
    report = Discord.pending_report(pending)

    completed = %{
      report
      | winner_discord_ids: [draft.data["winner"]],
        details: details,
        raw: Map.put(report.raw, :winner_reported_by, draft.discord_id)
    }

    case Sink.dispatch(completed, GamesSink) do
      :ok ->
        Repo.delete!(pending)
        {:saved, pending.external_id}

      {:error, _} ->
        Repo.rollback("Could not save the result. Your draft is still available; try again.")
    end
  end

  defp update(draft, pending, data) do
    draft = draft |> Ecto.Changeset.change(data: data) |> Repo.update!()
    {:ok, draft, pending}
  end

  defp authorize(nil, _actor),
    do:
      {:error,
       "I haven't seen an unfinished SpellBot game here. Use /won game:SB12345 to choose one."}

  defp authorize(pending, actor) do
    configured = Application.get_env(:the_gathering, Discord, [])[:guild_id]
    account = Accounts.get_user_by_discord_id(actor.discord_id)

    cond do
      actor.guild_id == "" or actor.guild_id != pending.guild_id ->
        {:error, "Use /won in the game's server."}

      configured not in [nil, ""] and to_string(configured) != actor.guild_id ->
        {:error, "Use /won in the bot's configured server."}

      account != nil and account.disabled_at != nil ->
        {:error, "Your account is disabled."}

      true ->
        authorize_participant(pending, actor)
    end
  end

  defp authorize_participant(pending, actor) do
    cond do
      not Enum.any?(players(pending), &(&1.discord_id == actor.discord_id)) ->
        {:error, "Only a participant can report this game."}

      Repo.exists?(
        from g in Game, where: g.source == "discord" and g.external_id == ^pending.external_id
      ) ->
        {:error, "This game has already been recorded. Edit it in The Gathering instead."}

      true ->
        :ok
    end
  end

  defp snapshot(pending) do
    :crypto.hash(
      :sha256,
      :erlang.term_to_binary(
        {pending.players, pending.played_at, pending.guild_id, pending.channel_id}
      )
    )
  end
end
