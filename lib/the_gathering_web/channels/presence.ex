defmodule TheGatheringWeb.Presence do
  @moduledoc false

  use Phoenix.Presence,
    otp_app: :the_gathering,
    pubsub_server: TheGathering.PubSub
end
