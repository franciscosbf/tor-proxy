use std::{num::ParseIntError, time::Duration};

use anyhow::Context;
use clap::Parser;
use tor_proxy::{CRATE_NAME, barrier::Barrier, proxy::Proxy, tunnel::TunnelClient};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

fn parse_duration(arg: &str) -> Result<Duration, ParseIntError> {
    let seconds = arg.parse()?;

    Ok(std::time::Duration::from_secs(seconds))
}

/// Tunnels HTTP communications through Tor network
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Port were proxy will listening on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
    /// GCRA limiter replenish interval in seconds (time it
    /// takes to replenish a single cell during exaustion)
    #[arg(short, long,value_parser = parse_duration, default_value = "4", verbatim_doc_comment)]
    repenish: Duration,
    /// GCRA limiter max burst size until triggered.
    #[arg(short, long, default_value_t = 100)]
    max_burst: u32,
}

fn init_tracing() {
    let layer = tracing_subscriber::fmt::layer();
    let filter = tracing_subscriber::filter::filter_fn(|metadata| {
        let target = metadata.target();
        let level = *metadata.level();

        target.starts_with(CRATE_NAME) && level <= tracing::Level::INFO
    });

    tracing_subscriber::registry()
        .with(layer.with_filter(filter))
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let barrier =
        Barrier::build(args.repenish, args.max_burst).context("Failed to build rate limiter")?;

    let tunnel_client = TunnelClient::bootstrap()
        .await
        .context("Failed to bootstrap Tor client")?;

    let proxy = Proxy::build(barrier, tunnel_client, args.port);

    init_tracing();

    proxy.run().await.context("Failed to run proxy")?;

    Ok(())
}
