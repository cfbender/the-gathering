# Every SFU peer connection generates an RSA-2048 DTLS certificate inside a regular (non-dirty)
# NIF in ex_dtls, which blocks a scheduler for 50-500 ms. Channel tests that start or stop
# seats around a push would otherwise miss ExUnit's default 100 ms assert_receive window.
ExUnit.start(assert_receive_timeout: 2_000)
Ecto.Adapters.SQL.Sandbox.mode(TheGathering.Repo, :manual)
