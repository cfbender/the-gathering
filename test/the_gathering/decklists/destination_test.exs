defmodule TheGathering.Decklists.DestinationTest do
  use ExUnit.Case, async: false

  alias TheGathering.Accounts.User
  alias TheGathering.Decklists.Destination

  setup do
    allowed_hosts = Application.get_env(:the_gathering, :manavault_allowed_hosts)
    allow_insecure = Application.get_env(:the_gathering, :manavault_allow_insecure_urls)
    resolver = Application.get_env(:the_gathering, :decklists_dns_resolver)

    Application.put_env(:the_gathering, :manavault_allowed_hosts, [])
    Application.put_env(:the_gathering, :manavault_allow_insecure_urls, false)

    on_exit(fn ->
      restore_env(:manavault_allowed_hosts, allowed_hosts)
      restore_env(:manavault_allow_insecure_urls, allow_insecure)
      restore_env(:decklists_dns_resolver, resolver)
    end)
  end

  test "accepts only normalized HTTPS origins" do
    assert {:ok, "https://vault.example.com:444"} =
             Destination.normalize_origin(" HTTPS://Vault.Example.COM:444/ ")

    for url <- [
          "https://user:pass@vault.example.com",
          "https://vault.example.com/api",
          "https://vault.example.com?admin=1",
          "https://vault.example.com#fragment",
          "https://vault.example.com:99999",
          "https://vault.example.com:not-a-port",
          "http://vault.example.com"
        ] do
      assert {:error, _message} = Destination.normalize_origin(String.trim(url))
    end
  end

  test "profile validation rejects credentials, paths, query strings, and fragments" do
    for url <- [
          "https://user@vault.example.com",
          "https://vault.example.com/api",
          "https://vault.example.com?q=1",
          "https://vault.example.com#fragment"
        ] do
      changeset =
        User.profile_changeset(%User{display_name: "User"}, %{
          "display_name" => "User",
          "manavault_url" => url
        })

      assert {"must be an allowed origin (scheme, host, and optional port only)", _} =
               changeset.errors[:manavault_url]
    end
  end

  test "blocks loopback, private, link-local, Tailscale, and IPv6 private destinations" do
    for address <- [
          "127.0.0.1",
          "10.0.0.1",
          "172.16.0.1",
          "192.168.1.1",
          "169.254.1.1",
          "100.64.0.1",
          "[::1]",
          "[fc00::1]",
          "[fe80::1]",
          "[::ffff:127.0.0.1]"
        ] do
      assert {:error, :blocked_destination} = Destination.resolve("https://#{address}")
    end
  end

  test "rejects a hostname if any DNS answer is private and observes changed answers" do
    {:ok, answers} = Agent.start_link(fn -> [[{93, 184, 216, 34}], [{127, 0, 0, 1}]] end)

    Application.put_env(:the_gathering, :decklists_dns_resolver, fn _host, family ->
      if family == :inet do
        Agent.get_and_update(answers, fn [next | rest] -> {{:ok, next}, rest} end)
      else
        {:ok, []}
      end
    end)

    assert {:ok, _uri, {93, 184, 216, 34}} = Destination.resolve("https://vault.example.com")
    assert {:error, :blocked_destination} = Destination.resolve("https://vault.example.com")
  end

  test "operator allowlist permits a private HTTP host" do
    Application.put_env(:the_gathering, :manavault_allowed_hosts, ["vault.internal"])

    Application.put_env(:the_gathering, :decklists_dns_resolver, fn _host, family ->
      if family == :inet, do: {:ok, [{192, 168, 1, 20}]}, else: {:ok, []}
    end)

    assert {:ok, "http://vault.internal:4000"} =
             Destination.normalize_origin("http://vault.internal:4000")

    assert {:ok, _uri, {192, 168, 1, 20}} = Destination.resolve("http://vault.internal:4000")
  end

  defp restore_env(key, nil), do: Application.delete_env(:the_gathering, key)
  defp restore_env(key, value), do: Application.put_env(:the_gathering, key, value)
end
