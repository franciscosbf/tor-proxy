use std::{net::SocketAddr, sync::Arc};

use arti_client::DataStream;
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::{HTTPS_PORT, tunnel::TunnelClient};

const LOCALHOST: [u8; 4] = [127, 0, 0, 1];

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TlsAcceptor(#[from] tokio_native_tls::native_tls::Error),
}

fn build_response(
    status: http::StatusCode,
    body: BoxBody<Bytes, hyper::Error>,
) -> hyper::Response<BoxBody<Bytes, hyper::Error>> {
    let mut response = hyper::Response::new(body);
    *response.status_mut() = status;

    response
}

fn build_empty_response(status: http::StatusCode) -> hyper::Response<BoxBody<Bytes, hyper::Error>> {
    let body = http_body_util::Empty::<Bytes>::new()
        .map_err(|e| match e {})
        .boxed();

    build_response(status, body)
}

fn build_full_response<C>(
    status: http::StatusCode,
    msg: C,
) -> hyper::Response<BoxBody<Bytes, hyper::Error>>
where
    C: Into<Bytes>,
{
    let body = http_body_util::Full::new(msg.into())
        .map_err(|e| match e {})
        .boxed();

    build_response(status, body)
}

fn tunnel(
    mut upstream: DataStream,
    request: hyper::Request<hyper::body::Incoming>,
) -> hyper::Response<BoxBody<Bytes, hyper::Error>> {
    tokio::spawn(async move {
        match hyper::upgrade::on(request).await.map(TokioIo::new) {
            Ok(mut upgraded) => {
                if let Err(e) = tokio::io::copy_bidirectional(&mut upgraded, &mut upstream).await {
                    tracing::warn!("Data tunneling has failed: {:?}", e);
                }
            }
            Err(e) => {
                tracing::warn!("Tunnel upgrade has failed: {}", e);
            }
        }
    });

    build_empty_response(http::StatusCode::OK)
}

async fn send_request(
    upstream: tokio_native_tls::TlsStream<DataStream>,
    request: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let upstream = TokioIo::new(upstream);

    let response = match hyper::client::conn::http1::Builder::new()
        .preserve_header_case(true)
        .title_case_headers(true)
        .handshake(upstream)
        .await
    {
        Ok((mut sender, connection)) => {
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    tracing::warn!("Failed to hold HTTP connection with upstream: {}", e);
                }
            });

            sender
                .send_request(request)
                .await?
                .map(|response| response.boxed())
        }
        Err(e) => {
            tracing::warn!("HTTP handshake with upstream has failed: {}", e);

            build_full_response(
                http::StatusCode::SERVICE_UNAVAILABLE,
                "connection with upstream didn't proceed as expected",
            )
        }
    };

    Ok(response)
}

struct ProxyHandler {
    tunnel_client: TunnelClient,
}

impl ProxyHandler {
    async fn handle(
        &self,
        request: hyper::Request<hyper::body::Incoming>,
    ) -> Result<hyper::Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        let authority = match request.uri().authority() {
            Some(authority) => authority,
            None => {
                let response =
                    build_full_response(http::StatusCode::BAD_REQUEST, "invalid address");

                return Ok(response);
            }
        };

        match authority.port_u16() {
            Some(port) if port != HTTPS_PORT => {
                let response = build_full_response(
                    http::StatusCode::BAD_REQUEST,
                    "proxy only accepts connections to port 443",
                );

                return Ok(response);
            }
            _ => (),
        }
        let host = authority.host().to_string();

        let response = match request.method() {
            &hyper::Method::CONNECT => match self.tunnel_client.connect(host.as_str()).await {
                Ok(upstream) => tunnel(upstream, request),
                Err(e) => {
                    tracing::warn!("Failed to connect to upstream without TLS: {}", e);

                    build_full_response(
                        http::StatusCode::SERVICE_UNAVAILABLE,
                        "failed to establish non-TLS connection with upstream",
                    )
                }
            },
            _ => match self.tunnel_client.connect_tls(host.as_str()).await {
                Ok(upstream) => send_request(upstream, request).await?,
                Err(e) => {
                    tracing::warn!("Failed to connect to upstream: {}", e);

                    build_full_response(
                        http::StatusCode::SERVICE_UNAVAILABLE,
                        "failed to establish connection with upstream",
                    )
                }
            },
        };

        Ok(response)
    }
}

pub struct Proxy {
    port: u16,
    handler: Arc<ProxyHandler>,
}

impl Proxy {
    pub fn build(tunnel_client: TunnelClient, port: u16) -> Self {
        let handler = Arc::new(ProxyHandler { tunnel_client });

        Self { port, handler }
    }

    pub async fn run(self) -> Result<(), ProxyError> {
        let addr = SocketAddr::from((LOCALHOST, self.port));

        let listener = TcpListener::bind(addr).await?;

        tracing::info!("Listening on port: {}", self.port);

        loop {
            let (tcp_stream, addr) = listener.accept().await?;

            tracing::info!("Received connection from {}", addr);

            let chandler = Arc::clone(&self.handler);

            tokio::spawn(async move {
                if let Err(e) = hyper::server::conn::http1::Builder::new()
                    .preserve_header_case(true)
                    .title_case_headers(true)
                    .serve_connection(
                        TokioIo::new(tcp_stream),
                        hyper::service::service_fn(|req| chandler.handle(req)),
                    )
                    .with_upgrades()
                    .await
                {
                    tracing::warn!("Failed to serve {}: {}", addr, e);
                }
            });
        }
    }
}
