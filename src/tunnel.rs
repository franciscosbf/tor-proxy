use std::{net::IpAddr, time::Duration};

use arti_client::{DataStream, TorAddr, TorClient, TorClientConfig};
use itertools::Itertools;
use moka::future::Cache;
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
    isolated_clients: Cache<String, TorClient<PreferredRuntime>>,
}

impl TunnelClient {
    pub async fn bootstrap(
        circuits: usize,
        max_entries: u64,
        ttl: Duration,
    ) -> Result<Self, TunnelClientError> {
        let mut config_builder = TorClientConfig::builder();
        config_builder
            .preemptive_circuits()
            .disable_at_threshold(circuits);
        let config = config_builder.build().unwrap();

        let tor_client = TorClient::create_bootstrapped(config).await?;
        let isolated_clients = Cache::builder()
            .max_capacity(max_entries)
            .time_to_live(ttl)
            .build();

        Ok(Self {
            tor_client,
            isolated_clients,
        })
    }

    pub async fn connect(&self, host: &str) -> Result<DataStream, TunnelClientError> {
        let addr = TorAddr::from((host, HTTPS_PORT))?;

        let host_base = host
            .parse::<IpAddr>()
            .map(|_| host.to_string())
            .unwrap_or_else(|_| dns_last_two_levels(host));

        let isolated_client = self
            .isolated_clients
            .get_with(host_base.clone(), async {
                self.tor_client.isolated_client()
            })
            .await;

        let data_stream = match isolated_client.connect(addr).await {
            Ok(data_stream) => data_stream,
            Err(e) => {
                self.isolated_clients.invalidate(&host_base).await;

                return Err(e.into());
            }
        };

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
