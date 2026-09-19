defmodule TheGathering.Decklists.HTTP do
  @moduledoc false

  @user_agent "TheGathering/0.1 deck metadata resolver (+https://github.com/cfbender/the-gathering)"

  def get(url) do
    request(method: :get, url: url)
  end

  def post(url, json) do
    request(method: :post, url: url, json: json)
  end

  defp request(options) do
    defaults = [
      headers: [{"accept", "application/json"}, {"user-agent", @user_agent}],
      connect_options: [timeout: 3_000],
      receive_timeout: 5_000,
      retry: false
    ]

    req_options = Application.get_env(:the_gathering, :decklists_req_options, [])
    Req.request(Keyword.merge(defaults, req_options) ++ options)
  end
end
