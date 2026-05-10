mod clipboard;
mod clipboard_monitor;
mod config;
mod container;
mod echo_guard;
mod message;
mod sync_engine;
mod transport;

use clap::Parser;
use config::Config;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "clipboard-share", about = "Bidirectional clipboard sync over TCP")]
struct Cli {
    /// Local address to listen on.
    #[arg(long, default_value = "0.0.0.0:9876")]
    listen: SocketAddr,

    /// Peer address to connect to.
    #[arg(long)]
    peer: SocketAddr,

    /// Delay between reconnect attempts in milliseconds.
    #[arg(long, default_value_t = 4000)]
    reconnect_delay_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("clipboard_share=info".parse()?),
        )
        .init();

    let cli = Cli::parse();
    let config = Config {
        listen: cli.listen,
        peer: cli.peer,
        reconnect_delay_ms: cli.reconnect_delay_ms,
    };

    container::run(config).await
}
