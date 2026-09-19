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
  end

  scope "/api", TheGatheringWeb.API do
    pipe_through :api

    get "/health", HealthController, :show
    get "/cards", CardController, :index
    get "/cards/:id", CardController, :show
    get "/catalog", CatalogController, :show
    post "/catalog/sync", CatalogController, :sync

    resources "/players", PlayerController, except: [:new, :edit]
    resources "/decks", DeckController, except: [:new, :edit]
    resources "/games", GameController, except: [:new, :edit]
    post "/decklists/resolve", DecklistController, :resolve

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
