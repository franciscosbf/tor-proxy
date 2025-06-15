use arti_client::{DataStream, TorAddr, TorClient, TorClientConfig};
use dashmap::DashMap;
use itertools::Itertools;
use safelog::Redactable;
use tor_proto::stream::ClientStreamCtrl;
use tor_rtcompat::PreferredRuntime;

use crate::HTTPS_PORT;

#[derive(Debug, thiserror::Error)]
pub enum TunnelClientError {
    #[error(transparent)]
    TorAddress(#[from] arti_client::TorAddrError),
    #[error(transparent)]
    TorClient(#[from] arti_client::Error),
    #[error("failed to inspect connection controller")]
    WithoutController,
    #[error("failed to retrieve tor circuilt")]
    WithoutCircuit,
    #[error(transparent)]
    TorProto(#[from] tor_proto::Error),
}

fn dns_last_two_levels(domain: &str) -> String {
    domain.split('.').rev().take(2).join(".")
}

pub struct TunnelClient {
    tor_client: TorClient<PreferredRuntime>,
    cached_clients: DashMap<String, TorClient<PreferredRuntime>>,
}

impl TunnelClient {
    pub async fn bootstrap(circuits: usize) -> Result<Self, TunnelClientError> {
        let mut config_builder = TorClientConfig::builder();
        config_builder
            .preemptive_circuits()
            .disable_at_threshold(circuits);
        let config = config_builder.build().unwrap();

        let tor_client = TorClient::create_bootstrapped(config).await?;
        let cached_clients = DashMap::new();

        Ok(Self {
            tor_client,
            cached_clients,
        })
    }

    pub async fn connect(&self, host: &str) -> Result<DataStream, TunnelClientError> {
        let addr = TorAddr::from((host, HTTPS_PORT))?;

        let host_base = dns_last_two_levels(host);

        let isolated_client = self
            .cached_clients
            .entry(host_base.clone())
            .or_insert_with(|| self.tor_client.isolated_client())
            .clone();

        let data_stream = isolated_client.connect(addr).await.inspect_err(|_| {
            self.cached_clients.remove(&host_base);
        })?;

        if tracing::enabled!(tracing::Level::DEBUG) {
            match data_stream
                .client_stream_ctrl()
                .and_then(|ctrl| ctrl.circuit())
            {
                Some(circuit) => match circuit.path_ref() {
                    Ok(circuit_path) => {
                        tracing::debug!(
                            "Connected to '{}' using circuit {}",
                            host,
                            circuit_path.iter().map(Redactable::redacted).join(", "),
                        );
                    }
                    Err(e) => {
                        tracing::debug!(
                            "Failed to retrive stream circuit path with {}: {}",
                            host,
                            e
                        );
                    }
                },
                None => {
                    tracing::debug!("Failed to retrieve stream circuit with {}", host)
                }
            }
        }

        Ok(data_stream)
    }
}
