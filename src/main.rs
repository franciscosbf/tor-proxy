use std::{num::ParseIntError, time::Duration};

use anyhow::Context;
use clap::Parser;
use tor_proxy::{
    CRATE_NAME,
    barrier::Barrier,
    proxy::{BufferSizes, Proxy},
    tunnel::TunnelClient,
};
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
    #[arg(short, long, value_parser = parse_duration, default_value = "4", verbatim_doc_comment)]
    repenish: Duration,
    /// GCRA limiter max burst size until triggered.
    #[arg(short, long, default_value_t = 100)]
    max_burst: u32,
    /// Connection buffer between user and proxy (in KiB).
    #[arg(short, long, default_value_t = 40)]
    incoming_buf: u32,
    /// Connection buffer between proxy and Tor network (in KiB).
    #[arg(short, long, default_value_t = 40)]
    outgoing_buf: u32,
    /// Increase tracing verbosity.
    #[arg(short, long)]
    debug: bool,
}

fn init_tracing(debug: bool) {
    let layer = tracing_subscriber::fmt::layer();
    let upper_level = if debug {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    let filter = tracing_subscriber::filter::filter_fn(move |metadata| {
        let target = metadata.target();
        let level = *metadata.level();

        target.starts_with(CRATE_NAME) && level <= upper_level
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

    let buffer_sizes = BufferSizes {
        outgoing_buf: args.outgoing_buf as usize * 1024,
        incoming_buf: args.incoming_buf as usize * 1024,
    };

    let proxy = Proxy::build(barrier, tunnel_client, args.port, buffer_sizes)
        .context("Failed to build proxy")?;

    init_tracing(args.debug);

    proxy.run().await.context("Failed to run proxy")?;

    Ok(())
}
