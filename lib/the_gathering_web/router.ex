defmodule TheGatheringWeb.Router do
  use TheGatheringWeb, :router

  pipeline :browser do
    plug :accepts, ["html"]
    plug :fetch_session
    plug :protect_from_forgery
    plug :put_secure_browser_headers
    plug TheGatheringWeb.UserAuth, :fetch_current_scope_for_user
  end

  # The SPA authenticates with the session cookie, so API mutations must carry
  # the CSRF token the shell embeds (`x-csrf-token`, sent by `lib/api.ts`).
  pipeline :api do
    plug :accepts, ["json"]
    plug :fetch_session
    plug :protect_from_forgery
    plug TheGatheringWeb.UserAuth, :fetch_current_scope_for_user
  end

  pipeline :require_authenticated_user do
    plug TheGatheringWeb.UserAuth, :require_authenticated_user
  end

  pipeline :require_admin do
    plug TheGatheringWeb.UserAuth, :require_admin
  end

  pipeline :require_sudo_mode do
    plug TheGatheringWeb.UserAuth, :require_sudo_mode
  end

  pipeline :rate_limit_credentials do
    plug TheGatheringWeb.RateLimit, bucket: :credentials
  end

  pipeline :rate_limit_sudo do
    plug TheGatheringWeb.RateLimit, bucket: :sudo
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through :api

    get "/health", HealthController, :show
    get "/registration", RegistrationController, :show
    get "/session", SessionController, :show
    delete "/session", SessionController, :delete
  end

  # Password login and bootstrap registration are the only public endpoints
  # that accept credentials, so throttle guessing per client address.
  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :rate_limit_credentials]

    post "/users", RegistrationController, :create
    post "/session", SessionController, :create
  end

  # Everything about the playgroup, including game history and the card
  # catalog, is private to signed-in members.
  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user]

    patch "/session/user", SessionController, :update_profile
    get "/session/remote-decks", RemoteDeckController, :index
    post "/session/remote-decks/sync", RemoteDeckController, :sync

    get "/cards", CardController, :index
    get "/cards/:id", CardController, :show
    get "/card-printings", CardPrintingController, :index
    get "/card-printings/:id", CardPrintingController, :show
    get "/catalog", CatalogController, :show
    get "/stats/overview", StatsController, :overview
    get "/stats/players/:id", StatsController, :player
    get "/stats/decks/:id", StatsController, :deck
    get "/stats/commanders", StatsController, :commanders
    get "/stats/commanders/:id", StatsController, :commander
    get "/deck-chooser", DeckChooserController, :show
    post "/deck-chooser/:id/outcomes", DeckChooserController, :create_outcome
    resources "/players", PlayerController, except: [:new, :edit]
    resources "/decks", DeckController, except: [:new, :edit]
    resources "/games", GameController, except: [:new, :edit]
    post "/decklists/resolve", DecklistController, :resolve
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user, :rate_limit_sudo]

    post "/session/sudo", SessionController, :sudo
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user, :require_admin]

    get "/imports/csv/sample", CSVImportController, :sample
    post "/imports/csv/preview", CSVImportController, :preview
    post "/imports/mythic_track/preview", MythicTrackImportController, :preview
    post "/imports/sheet/preview", SheetImportController, :preview
    get "/exports/portable", PortableImportController, :export
    post "/imports/portable/preview", PortableImportController, :preview
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user, :require_admin, :require_sudo_mode]

    post "/imports/csv", CSVImportController, :create
    post "/imports/mythic_track", MythicTrackImportController, :create
    post "/imports/sheet", SheetImportController, :create
    post "/imports/portable", PortableImportController, :create
    post "/players/:id/merge", PlayerController, :merge
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user, :require_sudo_mode]

    patch "/session/password", SessionController, :update_password
  end

  scope "/api/admin", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user, :require_admin, :require_sudo_mode]

    resources "/users", AdminUserController, only: [:index, :update, :delete]
    delete "/users/:id/sessions", AdminUserController, :revoke_sessions
    put "/users/:id/player", AdminUserController, :link_player
    get "/settings", AdminSettingsController, :show
    patch "/settings", AdminSettingsController, :update
    get "/discord/pending", AdminDiscordPendingController, :index
    patch "/discord/pending/:id", AdminDiscordPendingController, :update
    delete "/discord/pending/:id", AdminDiscordPendingController, :delete
    post "/catalog/sync", CatalogController, :sync
    post "/catalog/backfill", CatalogController, :backfill
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through :api

    # Keep unknown API paths out of the SPA catch-all below.
    match :*, "/*path", FallbackController, :not_found
  end

  # Everything that is not an API route or a static file is a client-side
  # route rendered by the React app.
  scope "/", TheGatheringWeb do
    pipe_through :browser

    get "/auth/discord", DiscordAuthController, :request
    get "/auth/discord/callback", DiscordAuthController, :callback
    get "/", AppController, :index
    get "/*path", AppController, :index
  end
end
