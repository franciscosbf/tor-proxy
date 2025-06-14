use std::{num::ParseIntError, str::FromStr, time::Duration};

use anyhow::Context;
use bytesize::ByteSize;
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

fn parse_byte_size(arg: &str) -> Result<ByteSize, String> {
    ByteSize::from_str(arg)
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
    /// Connection buffer between user and proxy.
    #[arg(short, long, value_parser = parse_byte_size, default_value = "512B")]
    incoming_buf: ByteSize,
    /// Connection buffer between proxy and Tor network.
    #[arg(short, long, value_parser = parse_byte_size, default_value = "512B")]
    outgoing_buf: ByteSize,
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
        Barrier::build(args.repenish, args.max_burst).context("failed to build rate limiter")?;

    let tunnel_client = TunnelClient::bootstrap()
        .await
        .context("failed to bootstrap Tor client")?;

    let buffer_sizes = BufferSizes {
        outgoing_buf: args.outgoing_buf.as_u64() as usize,
        incoming_buf: args.incoming_buf.as_u64() as usize,
    };

    let proxy = Proxy::build(barrier, tunnel_client, args.port, buffer_sizes)
        .context("failed to build proxy")?;

    init_tracing(args.debug);

    proxy.run().await.context("failed to run proxy")?;

    Ok(())
}
