# AGENTS.md

## Project Structure

The Gathering is a self-hosted Commander (Magic: The Gathering) game tracker: an Elixir/Phoenix JSON API plus a Vite/React single-page app, shipped as one container.

- `lib/the_gathering/` — core application/domain code (Ecto schemas, contexts, Scryfall catalog, imports).
- `lib/the_gathering_web/` — Phoenix web layer: router, `/api` controllers, the SPA shell (`AppController`), and `ViteAssets` (dev-server vs manifest asset tags).
- `config/` — Phoenix, runtime, database, and environment configuration. `runtime.exs` reads `DATA_DIR`, `DATABASE_PATH`, `SECRET_KEY_BASE`, `PHX_HOST`, `PHX_SCHEME`, `PHX_URL_PORT`.
- `priv/repo/` — Ecto migrations (SQLite).
- `assets/react/` — the React app (`src/routes` are TanStack Router file-based routes; `routeTree.gen.ts` is generated). Tailwind 4 + daisyUI themes live in `src/app.css`.
- `test/` — ExUnit tests and test support.
- `Dockerfile`, `docker-entrypoint.sh`, `docker-compose.yml` — production container build and startup flow.
- `mise.toml` — pinned toolchain (Erlang, Elixir, Node, aube).
- `.agents/setup` and `.agents/resume` — orb bootstrap scripts; `.amp/services.yaml` — the review portal service.

## Common Commands

Run commands through `mise` to use the pinned toolchain:

```sh
mise install
mise exec -- mix setup          # deps, database, first asset build
mise exec -- mix phx.server     # Phoenix on $PORT (default 4000) + Vite dev server on 5173
mise exec -- mix test
mise exec -- mix precommit      # compile --warnings-as-errors, format, credo, tests, aube run precommit
```

JavaScript tooling goes through aube (the package manager) and Vite Plus (`vp`):

```sh
mise exec -- aube install --frozen-lockfile
mise exec -- aube run build             # production bundle into priv/static/assets/react
mise exec -- aube exec vp check         # fmt + lint + typecheck
mise exec -- aube exec vp test run
```

Use `mise exec -- aube` instead of invoking `aube` or npm directly. Fresh orbs do not expose aube on `PATH`. Note that `vp check` is a Vite Plus built-in, so call it via `aube exec vp check` rather than the npm script.

In development the Vite dev server (port 5173) is the browser entry point; it proxies everything except its own assets to Phoenix. In an orb, `.amp/services.yaml` already runs this stack as the `the-gathering-review` service, so check `amp orb service status the-gathering-review` (or `ss -ltnp`) before starting another Phoenix server, and reuse the existing one.

After creating a new Ecto migration, run it before reporting the change complete:

```sh
mise exec -- mix ecto.migrate
```

Production/container commands are documented in `README.md`.

## Development Notes

- The backend is API-only (`--no-html --no-live`): there are no LiveViews, layouts, or `core_components`. Render UI in React; serve data from `/api` controllers.

### JSON API conventions

- Controllers live in `lib/the_gathering_web/controllers/api/` under the `TheGatheringWeb.API` namespace and are routed inside the `scope "/api", TheGatheringWeb.API` block. Add new routes above the catch-all `match :*` line.
- Every API controller declares `action_fallback TheGatheringWeb.API.FallbackController` and returns `{:error, %Ecto.Changeset{}}`, `{:error, :not_found}`, `{:error, :unauthorized}`, `{:error, :forbidden}`, or `{:error, :bad_request}` from actions instead of rendering errors by hand. Changeset errors render as `{"errors": {"field": ["message"]}}`; other errors as `{"errors": {"detail": "..."}}`.
- Successful responses wrap the payload in `{"data": ...}` (single object or list). Render JSON with a `*JSON` module next to the controller (for example `GameJSON.show/1`, `GameJSON.index/1`) rather than building maps inline.
- Use plural resource paths and standard REST actions (`GET /api/games`, `POST /api/games`, `GET /api/games/:id`, `PATCH`, `DELETE`). Paginate lists with `page`/`per_page` query params when they can grow unbounded.
- The `/api` pipeline runs `protect_from_forgery`; the frontend `api()` helper in `assets/react/src/lib/api.ts` sends the CSRF token and rejects with `ApiError` (carrying `errors`) on non-2xx responses. Use it for all requests.
- Server state in React goes through TanStack Query (`useQuery`/`useMutation`, `QueryClientProvider` in `main.tsx`; the `queryClient` is also in router context). Key queries by resource, for example `["games", id]`.
- Use `Req` for HTTP requests (Scryfall, Discord, deck-list sites). Avoid `:httpoison`, `:tesla`, and `:httpc`.
- Follow existing Phoenix context and React component patterns. Keep changes small and focused.
- Frontend styling uses Tailwind utilities and daisyUI component classes; theme tokens are defined in `assets/react/src/app.css`. Use `cn()` from `src/lib/cn.ts` to merge classes.
- Run the narrowest relevant tests before reporting completion, and `mise exec -- mix precommit` when a change is complete.
- For UI changes, verify the rendered result through the review portal and leave the service running.
- Update documentation when project structure, setup, or runtime behavior changes.

## Git Commit Policy

- Every Git commit must use a Conventional Commits message.
- Commit as the current thread's user using their configured Git identity.
- Never add `Co-authored-by` trailers or credit Amp, an AI agent, or another co-author.
- Never push unless the user asks. When they do, verify the commit has no co-authorship trailers, then push the current branch and confirm it matches its upstream.

<!-- usage-rules-start -->

<!-- phoenix:elixir-start -->
## Elixir guidelines

- Elixir lists **do not support index based access via the access syntax**

  **Never do this (invalid)**:

      i = 0
      mylist = ["blue", "green"]
      mylist[i]

  Instead, **always** use `Enum.at`, pattern matching, or `List` for index based list access, ie:

      i = 0
      mylist = ["blue", "green"]
      Enum.at(mylist, i)

- Elixir variables are immutable, but can be rebound, so for block expressions like `if`, `case`, `cond`, etc
  you *must* bind the result of the expression to a variable if you want to use it and you CANNOT rebind the result inside the expression, ie:

      # INVALID: we are rebinding inside the `if` and the result never gets assigned
      if connected?(socket) do
        socket = assign(socket, :val, val)
      end

      # VALID: we rebind the result of the `if` to a new variable
      socket =
        if connected?(socket) do
          assign(socket, :val, val)
        end

- **Never** nest multiple modules in the same file as it can cause cyclic dependencies and compilation errors
- **Never** use map access syntax (`changeset[:field]`) on structs as they do not implement the Access behaviour by default. For regular structs, you **must** access the fields directly, such as `my_struct.field` or use higher level APIs that are available on the struct if they exist, `Ecto.Changeset.get_field/2` for changesets
- Elixir's standard library has everything necessary for date and time manipulation. Familiarize yourself with the common `Time`, `Date`, `DateTime`, and `Calendar` interfaces by accessing their documentation as necessary. **Never** install additional dependencies unless asked or for date/time parsing (which you can use the `date_time_parser` package)
- Don't use `String.to_atom/1` on user input (memory leak risk)
- Predicate function names should not start with `is_` and should end in a question mark. Names like `is_thing` should be reserved for guards
- Elixir's builtin OTP primitives like `DynamicSupervisor` and `Registry`, require names in the child spec, such as `{DynamicSupervisor, name: MyApp.MyDynamicSup}`, then you can use `DynamicSupervisor.start_child(MyApp.MyDynamicSup, child_spec)`
- Use `Task.async_stream(collection, callback, options)` for concurrent enumeration with back-pressure. The majority of times you will want to pass `timeout: :infinity` as option

## Mix guidelines

- Read the docs and options before using tasks (by using `mix help task_name`)
- To debug test failures, run tests in a specific file with `mix test test/my_test.exs` or run all previously failed tests with `mix test --failed`
- `mix deps.clean --all` is **almost never needed**. **Avoid** using it unless you have good reason

## Test guidelines

- **Always use `start_supervised!/1`** to start processes in tests as it guarantees cleanup between tests
- **Avoid** `Process.sleep/1` and `Process.alive?/1` in tests
  - Instead of sleeping to wait for a process to finish, **always** use `Process.monitor/1` and assert on the DOWN message:

      ref = Process.monitor(pid)
      assert_receive {:DOWN, ^ref, :process, ^pid, :normal}

   - Instead of sleeping to synchronize before the next call, **always** use `_ = :sys.get_state/1` to ensure the process has handled prior messages
<!-- phoenix:elixir-end -->

<!-- phoenix:phoenix-start -->
## Phoenix guidelines

- Remember Phoenix router `scope` blocks include an optional alias which is prefixed for all routes within the scope. **Always** be mindful of this when creating routes within a scope to avoid duplicate module prefixes.

- You **never** need to create your own `alias` for route definitions! The `scope` provides the alias, ie:

      scope "/admin", AppWeb.Admin do
        pipe_through :browser

        live "/users", UserLive, :index
      end

  the UserLive route would point to the `AppWeb.Admin.UserLive` module

- `Phoenix.View` no longer is needed or included with Phoenix, don't use it
<!-- phoenix:phoenix-end -->

<!-- phoenix:ecto-start -->
## Ecto Guidelines

- **Always** preload Ecto associations in queries when they'll be accessed in templates, ie a message that needs to reference the `message.user.email`
- Remember `import Ecto.Query` and other supporting modules when you write `seeds.exs`
- `Ecto.Schema` fields always use the `:string` type, even for `:text`, columns, ie: `field :name, :string`
- `Ecto.Changeset.validate_number/2` **DOES NOT SUPPORT the `:allow_nil` option**. By default, Ecto validations only run if a change for the given field exists and the change value is not nil, so such as option is never needed
- You **must** use `Ecto.Changeset.get_field(changeset, :field)` to access changeset fields
- Fields which are set programmatically, such as `user_id`, must not be listed in `cast` calls or similar for security purposes. Instead they must be explicitly set when creating the struct
- **Always** invoke `mix ecto.gen.migration migration_name_using_underscores` when generating migration files, so the correct timestamp and conventions are applied
<!-- phoenix:ecto-end -->

