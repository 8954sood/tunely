mod config;
mod http_ingress;
mod state;
mod ws_session;

use anyhow::Context;
use axum::{routing::any, routing::get, Router};
use clap::Parser;
use tracing::info;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "relay_server=info,info".into()),
        )
        .init();

    let config = Config::parse();
    let state = AppState::new(config.auth.clone(), config.request_timeout_secs);

    let app = Router::new()
        .route("/ws", get(ws_session::ws_handler))
        .route("/t/:tunnel_id", any(http_ingress::ingress_root))
        .route("/t/:tunnel_id/", any(http_ingress::ingress_root))
        .route("/t/:tunnel_id/*path", any(http_ingress::ingress_path))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("failed to bind {}", config.listen))?;

    info!(listen = %config.listen, "relay server listening");
    axum::serve(listener, app)
        .await
        .context("relay server failed")
}
