defmodule TheGathering.WebcamTables.Sfu.BrowserSdp do
  @moduledoc """
  Smooths over browser SDP that `ExWebRTC` rejects but the browser means harmlessly.

  Firefox and Safari answer the server's re-offers with their original DTLS role
  (`a=setup:passive`) on the media sections they already had and `a=setup:active` on the ones
  the offer added. All of those sections share one bundled transport, so the role of the
  BUNDLE-tagged section is the only one that counts (RFC 8843 §7.1), but `ExWebRTC` insists
  that every section agree and otherwise fails with `:conflicting_dtls_roles`. Copying the
  tagged section's role onto the others gives it the SDP it accepts without changing what the
  browser will do.
  """

  @doc "The description with every media section's `a=setup` set to the bundled transport's."
  @spec unify_dtls_roles(String.t()) :: String.t()
  def unify_dtls_roles(sdp) do
    {session, sections} = split_sections(sdp)

    case role_of(session, sections) do
      nil -> sdp
      role -> Enum.join([session | Enum.map(sections, &set_role(&1, role))])
    end
  end

  # The session lines, then each media section, each keeping its own line breaks.
  defp split_sections(sdp) do
    [session | sections] = String.split(sdp, ~r/(?=^m=)/m)
    {session, sections}
  end

  defp role_of(session, sections) do
    tagged =
      case Regex.run(~r/^a=group:BUNDLE\s+(\S+)/m, session) do
        [_line, mid] -> Enum.find(sections, &(mid(&1) == mid))
        nil -> nil
      end

    Enum.find_value([tagged | sections], fn
      nil -> nil
      section -> setup(section)
    end)
  end

  defp mid(section) do
    case Regex.run(~r/^a=mid:(\S+)/m, section) do
      [_line, mid] -> mid
      nil -> nil
    end
  end

  defp setup(section) do
    case Regex.run(~r/^a=setup:(\S+)/m, section) do
      [_line, role] -> role
      nil -> nil
    end
  end

  defp set_role(section, role) do
    Regex.replace(~r/^a=setup:\S+/m, section, "a=setup:#{role}")
  end
end
