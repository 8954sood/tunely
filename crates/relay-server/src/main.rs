mod config;
mod http_ingress;
mod ingress;
mod state;
mod ws_session;
mod ws_tunnel;
mod ws_wire;

use anyhow::Context;
use axum::{Router, routing::any, routing::get};
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

    let config = Config::parse().resolve()?;
    let state = AppState::new(config.auth_tokens.clone(), config.request_timeout_secs);

    let app = Router::new()
        .route("/ws", get(ws_session::ws_handler))
        .route("/t/:tunnel_id", any(ingress::ingress_root))
        .route("/t/:tunnel_id/", any(ingress::ingress_root))
        .route("/t/:tunnel_id/*path", any(ingress::ingress_path))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("failed to bind {}", config.listen))?;

    info!(listen = %config.listen, "relay server listening");
    axum::serve(listener, app)
        .await
        .context("relay server failed")
}
