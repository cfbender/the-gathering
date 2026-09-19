defmodule TheGathering.Discord.Sink do
  @moduledoc "Destination for normalized Discord game reports."

  alias TheGathering.Discord.GameReport

  @callback handle_report(GameReport.t()) :: :ok | {:error, term()}

  @spec dispatch(GameReport.t(), module() | nil) :: :ok | {:error, term()}
  def dispatch(report, sink \\ nil) do
    sink = sink || Application.get_env(:the_gathering, :discord_sink, __MODULE__.Logger)
    sink.handle_report(report)
  end
end
