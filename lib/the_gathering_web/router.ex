defmodule TheGatheringWeb.Router do
  use TheGatheringWeb, :router

  pipeline :browser do
    plug :accepts, ["html"]
    plug :fetch_session
    plug :protect_from_forgery
    plug :put_secure_browser_headers
  end

  pipeline :api do
    plug :accepts, ["json"]
    plug :fetch_session
  end

  scope "/api", TheGatheringWeb do
    pipe_through :api

    get "/health", HealthController, :show

    # Keep unknown API paths out of the SPA catch-all below.
    match :*, "/*path", ApiFallbackController, :not_found
  end

  # Everything that is not an API route or a static file is a client-side
  # route rendered by the React app.
  scope "/", TheGatheringWeb do
    pipe_through :browser

    get "/", AppController, :index
    get "/*path", AppController, :index
  end
end
