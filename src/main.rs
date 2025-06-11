use anyhow::Context;
use clap::Parser;
use tor_proxy::{CRATE_NAME, proxy::Proxy, tunnel::TunnelClient};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

/// Tunnels HTTP communications through Tor network.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Port were proxy will listening to.
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
}

fn init_tracing() {
    let layer = tracing_subscriber::fmt::layer();
    let filter = tracing_subscriber::filter::filter_fn(|metadata| {
        let target = metadata.target();
        let level = *metadata.level();

        target.starts_with(CRATE_NAME) && level >= tracing::Level::INFO
    });

    tracing_subscriber::registry()
        .with(layer.with_filter(filter))
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let tunnel_client = TunnelClient::bootstrap()
        .await
        .context("Failed to bootstrap Tor client")?;

    let proxy = Proxy::build(tunnel_client, args.port);

    init_tracing();

    proxy.run().await.context("Failed to run proxy")?;

    Ok(())
}
