defmodule TheGathering.WebcamTables.Sfu do
  @moduledoc """
  The webcam table's selective forwarding unit.

  Every browser at a table holds one WebRTC connection to this server instead of one to
  each other seat. It publishes its camera once, as three simulcast layers, and receives
  one stream per other seat; `TheGathering.WebcamTables.Sfu.Room` picks the layer each
  viewer gets from how large that board is drawn, so a rail tile costs a quarter-resolution
  decode while the pinned board stays sharp.

  Rooms are started on demand under `TheGathering.WebcamTables.Sfu.RoomSupervisor` and
  registered by table id in `TheGathering.WebcamTables.Sfu.Registry`. Every call here is
  made from the joined channel process, which the room monitors.

  ## Network configuration (`config :the_gathering, :sfu`)

    * `:port_range` — UDP ports the server listens on for media; forward them to this host.
    * `:public_ip` — the address players reach those ports at (the router's WAN address).
      Without it the server offers only its own interface addresses, which works on a LAN.
    * `:relay_only` — connect through the configured Cloudflare TURN relay instead of
      listening for inbound UDP, for hosts that cannot forward ports.
  """

  alias TheGathering.CloudflareTurn
  alias TheGathering.WebcamTables.Sfu.Room

  @layers ["l", "m", "h"]

  @doc "Simulcast layer names, lowest resolution first; the browser encodes them in this order."
  def layers, do: @layers

  @doc "Registers the calling channel process as `peer_id` at `room_id`'s SFU."
  def join(room_id, peer_id, spectator?), do: join(room_id, peer_id, spectator?, 3)

  # A room stops as soon as its last seat leaves, so the pid found for a table whose
  # previous seat is just leaving may be gone by the time the call lands; start another.
  defp join(room_id, peer_id, spectator?, attempts) do
    GenServer.call(ensure_room(room_id), {:join, peer_id, self(), spectator?})
  catch
    :exit, {reason, _call} when reason in [:normal, :noproc] and attempts > 1 ->
      join(room_id, peer_id, spectator?, attempts - 1)
  end

  @doc "Applies the browser's initial offer and returns the answer SDP."
  def offer(room_id, peer_id, sdp), do: call(room_id, {:offer, peer_id, sdp})

  @doc "Applies the browser's answer to a server offer."
  def answer(room_id, peer_id, sdp), do: call(room_id, {:answer, peer_id, sdp})

  @doc "Adds an ICE candidate (as its JSON map) from the browser."
  def candidate(room_id, peer_id, candidate), do: call(room_id, {:candidate, peer_id, candidate})

  @doc "Asks for `layer` of `owner_id`'s video, from how large `peer_id` draws it."
  def layer(room_id, peer_id, owner_id, layer) when layer in @layers,
    do: call(room_id, {:layer, peer_id, owner_id, layer})

  @doc "Limits `peer_id`'s video to `target` (another peer id), or to everyone with `nil`."
  def reveal(room_id, peer_id, target), do: call(room_id, {:reveal, peer_id, target})

  @doc "Delivers `message` to `to`'s channel process as `{:sfu, :peer_message, payload}`."
  def relay(room_id, from, to, message), do: call(room_id, {:relay, from, to, message})

  @doc "What the browser needs to know: whether media reaches the server directly or via TURN."
  def client_info do
    %{transport: if(relay_only?(), do: "relay", else: "direct")}
  end

  @doc false
  def peer_connection_options do
    config = config()

    network =
      if relay_only?() do
        [ice_transport_policy: :relay, ice_servers: relay_servers()]
      else
        [
          ice_servers: [],
          ice_port_range: Keyword.get(config, :port_range, 50_000..50_100),
          ice_ip_filter: ip_filter(Keyword.get(config, :ipv6, false)),
          host_to_srflx_ip_mapper: public_ip_mapper(Keyword.get(config, :public_ip))
        ]
      end

    # H.264 lets hardware encoders and decoders carry the load in every browser; VP8 is the
    # fallback every WebRTC stack ships. No audio: the table is video-only.
    [video_codecs: [:h264, :vp8], audio_codecs: [], controlling_process: self()] ++ network
  end

  @doc false
  def valid_layer?(layer), do: layer in @layers

  defp relay_only? do
    Keyword.get(config(), :relay_only, false) and CloudflareTurn.configured?()
  end

  defp relay_servers do
    case CloudflareTurn.ice_servers() do
      {:ok, servers} ->
        for %{"urls" => urls} = server <- servers, server["username"] do
          %{urls: urls, username: server["username"], credential: server["credential"]}
        end

      {:error, _reason} ->
        []
    end
  end

  # IPv4 only unless asked: the port forward and WEBRTC_SFU_PUBLIC_IP are IPv4, browsers hide
  # their IPv6 host addresses behind mDNS names ex_ice resolves only to IPv4, and a LAN
  # browser that picks the server's IPv6 candidate has been seen to lose its media on it.
  defp ip_filter(true), do: fn _ip -> true end
  defp ip_filter(_ipv4_only), do: fn ip -> tuple_size(ip) == 4 end

  defp public_ip_mapper(nil), do: nil

  defp public_ip_mapper(public_ip) when is_binary(public_ip) do
    case :inet.parse_address(String.to_charlist(public_ip)) do
      {:ok, address} -> fn _host_ip -> address end
      {:error, _reason} -> nil
    end
  end

  defp config, do: Application.get_env(:the_gathering, :sfu, [])

  defp ensure_room(room_id) do
    case DynamicSupervisor.start_child(__MODULE__.RoomSupervisor, {Room, room_id}) do
      {:ok, pid} -> pid
      {:error, {:already_started, pid}} -> pid
    end
  end

  defp call(room_id, message) do
    case Registry.lookup(__MODULE__.Registry, room_id) do
      [{pid, _value}] -> GenServer.call(pid, message)
      [] -> {:error, :not_joined}
    end
  end
end
