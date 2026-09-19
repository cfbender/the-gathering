defmodule TheGatheringWeb.Router do
  use TheGatheringWeb, :router

  pipeline :browser do
    plug :accepts, ["html"]
    plug :fetch_session
    plug :protect_from_forgery
    plug :put_secure_browser_headers
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

  scope "/api", TheGatheringWeb.API do
    pipe_through :api

    get "/health", HealthController, :show
    get "/registration", RegistrationController, :show
    post "/users", RegistrationController, :create
    get "/session", SessionController, :show
    post "/session", SessionController, :create
    delete "/session", SessionController, :delete

    # Game history and the card catalog are readable without an account.
    get "/cards", CardController, :index
    get "/cards/:id", CardController, :show
    get "/catalog", CatalogController, :show
    resources "/players", PlayerController, only: [:index, :show]
    resources "/decks", DeckController, only: [:index, :show]
    resources "/games", GameController, only: [:index, :show]
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user]

    patch "/session/user", SessionController, :update_profile
    post "/session/sudo", SessionController, :sudo

    resources "/players", PlayerController, only: [:create, :update, :delete]
    resources "/decks", DeckController, only: [:create, :update, :delete]
    resources "/games", GameController, only: [:create, :update, :delete]
    post "/decklists/resolve", DecklistController, :resolve
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user, :require_sudo_mode]

    patch "/session/password", SessionController, :update_password
  end

  scope "/api/admin", TheGatheringWeb.API do
    pipe_through [:api, :require_authenticated_user, :require_admin, :require_sudo_mode]

    resources "/users", AdminUserController, only: [:index, :create, :update, :delete]
    patch "/users/:id/password", AdminUserController, :reset_password
    get "/settings", AdminSettingsController, :show
    patch "/settings", AdminSettingsController, :update
    post "/catalog/sync", CatalogController, :sync
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

    get "/", AppController, :index
    get "/*path", AppController, :index
  end
end
