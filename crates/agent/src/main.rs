mod config;
mod inflight;
mod local_proxy;
mod relay_client;

use clap::Parser;
use tracing::error;

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agent=info,info".into()),
        )
        .init();

    let config = Config::parse();
    if let Err(err) = relay_client::run(config).await {
        error!(error = %err, "agent terminated");
        return Err(err);
    }

    Ok(())
}
