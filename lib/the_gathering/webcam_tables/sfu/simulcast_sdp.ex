defmodule TheGathering.WebcamTables.Sfu.SimulcastSdp do
  @moduledoc """
  Keeps a browser's simulcast alive across the server's offers.

  The browser's first offer declares its camera's layers (`a=rid:… send` and
  `a=simulcast:send …`), and `ExWebRTC` reverses them in its answer. Its later offers do not
  repeat them, and a browser that is offered its camera's media section without them stops
  sending every layer but the first. The server does not need the lines itself (it demuxes
  layers by the RTP rid extension), so they are added only to the copy of the offer that
  goes to the browser.
  """

  alias ExWebRTC.SDPUtils

  @typedoc "Simulcast attributes for the server's side of a media section, keyed by its mid."
  @type attrs_by_mid :: %{String.t() => [ExSDP.Attribute.t()]}

  @doc "The `recv` counterpart of each simulcast-sending media section in a browser offer."
  @spec receiving(String.t()) :: attrs_by_mid()
  def receiving(sdp) do
    case ExSDP.parse(sdp) do
      {:ok, %ExSDP{media: media}} ->
        Map.new(
          for mline <- media,
              {:mid, mid} <- [ExSDP.get_attribute(mline, :mid)],
              attrs = SDPUtils.reverse_simulcast(mline),
              attrs != [],
              do: {mid, attrs}
        )

      {:error, _reason} ->
        %{}
    end
  end

  @doc "The server offer with `attrs` added to each media section whose mid they are for."
  @spec restore(String.t(), attrs_by_mid()) :: String.t()
  def restore(sdp, attrs) when map_size(attrs) == 0, do: sdp

  def restore(sdp, attrs) do
    case ExSDP.parse(sdp) do
      {:ok, %ExSDP{media: media} = parsed} ->
        to_string(%{parsed | media: Enum.map(media, &restore_mline(&1, attrs))})

      {:error, _reason} ->
        sdp
    end
  end

  defp restore_mline(mline, attrs) do
    case ExSDP.get_attribute(mline, :mid) do
      {:mid, mid} when is_map_key(attrs, mid) -> ExSDP.add_attributes(mline, attrs[mid])
      _other -> mline
    end
  end
end
