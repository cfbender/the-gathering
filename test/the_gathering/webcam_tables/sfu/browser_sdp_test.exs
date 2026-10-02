defmodule TheGathering.WebcamTables.Sfu.BrowserSdpTest do
  use ExUnit.Case, async: true

  alias TheGathering.WebcamTables.Sfu.BrowserSdp

  defp sdp(bundle, sections) do
    session = """
    v=0
    o=mozilla...THIS_IS_SDPARTA-99.0 1 2 IN IP4 0.0.0.0
    s=-
    t=0 0
    a=group:BUNDLE #{bundle}
    a=fingerprint:sha-256 AA:BB
    """

    String.replace(session <> Enum.join(sections), "\n", "\r\n")
  end

  defp section(mid, setup) do
    """
    m=video 9 UDP/TLS/RTP/SAVPF 120
    c=IN IP4 0.0.0.0
    a=mid:#{mid}
    #{if setup, do: "a=setup:#{setup}\n", else: ""}a=rtpmap:120 VP8/90000
    """
  end

  defp roles(sdp), do: Regex.scan(~r/^a=setup:(\S+)/m, sdp) |> Enum.map(&List.last/1)

  test "copies the BUNDLE-tagged section's role onto the others" do
    firefox_answer =
      sdp("0 1 2", [section("0", "passive"), section("1", "active"), section("2", "active")])

    unified = BrowserSdp.unify_dtls_roles(firefox_answer)

    assert roles(unified) == ["passive", "passive", "passive"]

    assert String.replace(unified, ~r/^a=setup:\S+/m, "") ==
             String.replace(firefox_answer, ~r/^a=setup:\S+/m, "")
  end

  test "follows the tag rather than the first section" do
    answer = sdp("1 0", [section("0", "active"), section("1", "passive")])

    assert roles(BrowserSdp.unify_dtls_roles(answer)) == ["passive", "passive"]
  end

  test "falls back to the first role when the tagged section has none" do
    answer = sdp("0 1", [section("0", nil), section("1", "active")])

    assert roles(BrowserSdp.unify_dtls_roles(answer)) == ["active"]
  end

  test "leaves consistent and role-less descriptions alone" do
    chrome_answer = sdp("0 1", [section("0", "active"), section("1", "active")])
    assert BrowserSdp.unify_dtls_roles(chrome_answer) == chrome_answer

    bare = sdp("0", [section("0", nil)])
    assert BrowserSdp.unify_dtls_roles(bare) == bare

    assert BrowserSdp.unify_dtls_roles("not sdp") == "not sdp"
  end
end
