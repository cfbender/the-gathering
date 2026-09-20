defmodule TheGathering.Decklists.Destination do
  @moduledoc false

  import Bitwise

  @type address :: :inet.ip_address()

  def normalize_origin(value) when is_binary(value) do
    value = String.trim(value)

    with {:ok, uri} <- URI.new(value),
         true <- uri.scheme in ["http", "https"],
         true <- is_binary(uri.host) and uri.host != "",
         true <- uri.port in 1..65_535,
         true <- is_nil(uri.userinfo),
         true <- uri.path in [nil, "", "/"],
         true <- is_nil(uri.query),
         true <- is_nil(uri.fragment),
         true <- uri.scheme == "https" or insecure_allowed?(uri.host) do
      {:ok, URI.to_string(%{uri | host: String.downcase(uri.host), path: nil})}
    else
      _invalid -> {:error, "must be an allowed origin (scheme, host, and optional port only)"}
    end
  end

  def normalize_origin(_value),
    do: {:error, "must be an allowed origin (scheme, host, and optional port only)"}

  def resolve(origin) do
    uri = URI.parse(origin)

    with {:ok, addresses} <- resolve_host(uri.host),
         true <- addresses != [],
         true <- allowed_host?(uri.host) or Enum.all?(addresses, &public_address?/1) do
      {:ok, uri, hd(addresses)}
    else
      _ -> {:error, :blocked_destination}
    end
  end

  def allowed_host?(host) when is_binary(host) do
    String.downcase(host) in Application.get_env(:the_gathering, :manavault_allowed_hosts, [])
  end

  defp insecure_allowed?(host) do
    Application.get_env(:the_gathering, :manavault_allow_insecure_urls, false) or
      allowed_host?(host)
  end

  defp resolve_host(host) do
    case :inet.parse_address(String.to_charlist(host)) do
      {:ok, address} ->
        {:ok, [address]}

      {:error, :einval} ->
        resolver = Application.get_env(:the_gathering, :decklists_dns_resolver, &:inet.getaddrs/2)

        addresses =
          [:inet, :inet6]
          |> Enum.flat_map(&resolve_family(resolver, host, &1))
          |> Enum.uniq()

        {:ok, addresses}
    end
  end

  defp resolve_family(resolver, host, family) do
    case resolver.(String.to_charlist(host), family) do
      {:ok, values} -> values
      {:error, _reason} -> []
    end
  end

  defp public_address?({0, _b, _c, _d}), do: false
  defp public_address?({10, _b, _c, _d}), do: false
  defp public_address?({100, b, _c, _d}) when b in 64..127, do: false
  defp public_address?({127, _b, _c, _d}), do: false
  defp public_address?({169, 254, _c, _d}), do: false
  defp public_address?({172, b, _c, _d}) when b in 16..31, do: false
  defp public_address?({192, 168, _c, _d}), do: false
  defp public_address?({a, _b, _c, _d}) when a >= 224, do: false
  defp public_address?({_a, _b, _c, _d}), do: true

  defp public_address?({0, 0, 0, 0, 0, 0, 0, 0}), do: false
  defp public_address?({0, 0, 0, 0, 0, 0, 0, 1}), do: false

  defp public_address?({0, 0, 0, 0, 0, 65_535, high, low}) do
    public_address?({high >>> 8, high &&& 255, low >>> 8, low &&& 255})
  end

  defp public_address?({a, _b, _c, _d, _e, _f, _g, _h}) when a in 0xFC00..0xFDFF,
    do: false

  defp public_address?({a, _b, _c, _d, _e, _f, _g, _h}) when a in 0xFE80..0xFEBF,
    do: false

  defp public_address?({a, _b, _c, _d, _e, _f, _g, _h}) when a in 0xFF00..0xFFFF,
    do: false

  defp public_address?({_a, _b, _c, _d, _e, _f, _g, _h}), do: true
end
