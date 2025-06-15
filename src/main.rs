use std::{collections::HashSet, num::ParseIntError, str::FromStr, time::Duration};

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

fn parse_byte_size(arg: &str) -> Result<usize, String> {
    let byte_size = ByteSize::from_str(arg)?;

    Ok(byte_size.as_u64() as usize)
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
    #[arg(short, long, value_parser = parse_duration, default_value = "4")]
    repenish: Duration,
    /// GCRA limiter max burst size until triggered.
    #[arg(long, default_value_t = 100)]
    max_burst: u32,
    /// Min number of Tor circuits established after client boostrap.
    #[arg(short, long, default_value_t = 12)]
    circuits: usize,
    /// Max capacity of cached Tor clients.
    #[arg(long, default_value_t = 100)]
    max_entries: u64,
    /// Time to live in seconds per cached Tor client.
    #[arg(short, long, value_parser = parse_duration, default_value = "3600")]
    ttl: Duration,
    /// Connection buffer between user and proxy.
    #[arg(short, long, value_parser = parse_byte_size, default_value = "512B")]
    incoming_buf: usize,
    /// Connection buffer between proxy and Tor network.
    #[arg(short, long, value_parser = parse_byte_size, default_value = "512B")]
    outgoing_buf: usize,
    /// Increase tracing verbosity.
    #[arg(short, long)]
    debug: bool,
}

fn init_tracing(debug: bool) {
    let layer = tracing_subscriber::fmt::layer();
    let allowed_traces = [
        CRATE_NAME,
        "tor_dirmgr",
        "tor_guardmgr",
        "tor_chanmgr",
        "tor_cirmgr",
    ]
    .into_iter()
    .collect::<HashSet<&'static str>>();
    let upper_level = if debug {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    let filter = tracing_subscriber::filter::filter_fn(move |metadata| {
        let base_target = metadata.target().split(':').next().unwrap();
        let level = *metadata.level();

        allowed_traces.contains(base_target) && level <= upper_level
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

    let tunnel_client = TunnelClient::bootstrap(args.circuits, args.max_entries, args.ttl)
        .await
        .context("failed to bootstrap Tor client")?;

    let buffer_sizes = BufferSizes {
        outgoing_buf: args.outgoing_buf,
        incoming_buf: args.incoming_buf,
    };

    let proxy = Proxy::build(barrier, tunnel_client, args.port, buffer_sizes)
        .context("failed to build proxy")?;

    init_tracing(args.debug);

    proxy.run().await.context("failed to run proxy")?;

    Ok(())
}
