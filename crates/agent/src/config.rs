use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "agent")]
pub struct Config {
    #[arg(long)]
    pub relay: String,
    #[arg(long)]
    pub tunnel_id: String,
    #[arg(long)]
    pub token: String,
    #[arg(long)]
    pub local: String,
    #[arg(long, default_value_t = 20)]
    pub ping_interval_secs: u64,
    #[arg(long, default_value_t = 30)]
    pub max_backoff_secs: u64,
}
