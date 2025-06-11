use arti_client::{DataStream, TorAddr, TorClient, TorClientConfig};
use tor_rtcompat::PreferredRuntime;

use crate::HTTPS_PORT;

#[derive(Debug, thiserror::Error)]
pub enum TunnelClientError {
    #[error(transparent)]
    TorAddress(#[from] arti_client::TorAddrError),
    #[error(transparent)]
    TorClient(#[from] arti_client::Error),
    #[error(transparent)]
    TlsConnector(#[from] tokio_native_tls::native_tls::Error),
}

pub struct TunnelClient {
    tor_client: TorClient<PreferredRuntime>,
}

impl TunnelClient {
    pub async fn bootstrap() -> Result<Self, TunnelClientError> {
        let config = TorClientConfig::default();
        let tor_client = TorClient::create_bootstrapped(config).await?;

        Ok(Self { tor_client })
    }

    pub async fn connect(&self, host: &str) -> Result<DataStream, TunnelClientError> {
        let isolated = self.tor_client.isolated_client();
        let addr = TorAddr::from((host, HTTPS_PORT))?;

        let data_stream = isolated.connect(addr).await?;

        Ok(data_stream)
    }

    pub async fn connect_tls(
        &self,
        host: &str,
    ) -> Result<tokio_native_tls::TlsStream<DataStream>, TunnelClientError> {
        let data_stream = self.connect(host).await?;

        let connector: tokio_native_tls::TlsConnector =
            tokio_native_tls::native_tls::TlsConnector::new()?.into();
        let tls_stream = connector.connect(host, data_stream).await?;

        Ok(tls_stream)
    }
}
