//! `the-gathering`: runs the server, or one-off maintenance commands.

use std::net::SocketAddr;

use anyhow::Context;
use the_gathering::config::Config;
use the_gathering::state::AppState;
use the_gathering::{catalog, db, decklists, discord, web};
use tracing_subscriber::EnvFilter;

fn usage() -> anyhow::Error {
    anyhow::anyhow!(
        "usage: the-gathering [serve | migrate | seed | create-admin USERNAME | bootstrap-admin | catalog-sync | catalog-backfill]"
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into());
    if !["debug", "info", "warning", "error"].contains(&level.as_str()) {
        anyhow::bail!("LOG_LEVEL must be one of debug, info, warning, error; got {level:?}");
    }
    let level = if level == "warning" {
        "warn".to_owned()
    } else {
        level
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("RUST_LOG")
                .unwrap_or_else(|_| EnvFilter::new(format!("{level},sqlx=warn"))),
        )
        // Container and journald logs are not terminals; keep escape codes out of them.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = Config::from_env()?;
    if let Some(parent) = config.database_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let pool = db::connect(&config.database_path, config.pool_size)
        .await
        .with_context(|| format!("opening {}", config.database_path.display()))?;
    let applied = db::migrate::run(&pool).await?;
    if !applied.is_empty() {
        tracing::info!("applied {} migration(s)", applied.len());
    }
    let state = AppState::new(config, pool)?;

    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => serve(state).await,
        ["migrate"] => Ok(()),
        ["seed"] => {
            if state.config.env == the_gathering::config::Env::Dev {
                println!("{}", the_gathering::seed::run(&state.games).await?);
            } else {
                println!("Skipping development demo data outside THE_GATHERING_ENV=dev.");
            }
            Ok(())
        }
        ["create-admin", username] => {
            let password = std::env::var("THE_GATHERING_ADMIN_PASSWORD")
                .context("THE_GATHERING_ADMIN_PASSWORD must be set")?;
            let user = state
                .accounts
                .create_admin(username, &password)
                .await
                .map_err(|error| anyhow::anyhow!("could not create admin: {error:?}"))?;
            println!("Created admin {}", user.username);
            Ok(())
        }
        ["bootstrap-admin"] => the_gathering::bootstrap_admin(&state).await,
        ["catalog-sync"] => {
            // A one-off sync; the scheduled sync never starts here.
            let count = catalog::sync::run(
                &state.pool,
                &state.scryfall,
                catalog::sync::Source::Scryfall,
            )
            .await
            .map_err(|error| anyhow::anyhow!("Catalog sync failed: {error}"))?;
            println!("Catalog synchronized: {count} cards");
            Ok(())
        }
        ["catalog-backfill"] => {
            let summary = catalog::backfill::run(&state.pool).await?;
            println!(
                "Split {} partner decks, linked {} commanders, filled {} color identities, linked {} MVP cards",
                summary.decks_split,
                summary.decks_linked,
                summary.colors_filled,
                summary.mvps_linked
            );
            for name in &summary.unmatched {
                println!("  unmatched: {name}");
            }
            Ok(())
        }
        _ => Err(usage()),
    }
}

async fn serve(state: AppState) -> anyhow::Result<()> {
    let address = SocketAddr::new(state.config.bind, state.config.port);
    if state.config.discord_oauth.is_some() {
        tracing::info!(
            "Discord OAuth sign-in enabled; redirect URI is {}/auth/discord/callback",
            state.config.public_url()
        );
    } else {
        tracing::info!(
            "Discord OAuth sign-in disabled: DISCORD_CLIENT_ID and DISCORD_CLIENT_SECRET are not both set"
        );
    }
    catalog::sync_server::start(&state);
    decklists::start_cache_sweeper(&state);
    if state.config.webcam_table_pruning_enabled {
        state.webcam_tables.spawn_pruner();
    }
    discord::start(&state);
    let app = web::router(state.clone());
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("binding {address}"))?;
    tracing::info!("listening on http://{address}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
