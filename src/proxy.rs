use std::{net::SocketAddr, sync::Arc};

use arti_client::DataStream;
use bytes::Bytes;
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::{HTTPS_PORT, barrier::Barrier, tunnel::TunnelClient};

const LOCALHOST: [u8; 4] = [127, 0, 0, 1];

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

#[derive(Clone, Copy)]
pub struct BufferSizes {
    pub outgoing_buf: usize,
    pub incoming_buf: usize,
}

struct ProxyHandler {
    tunnel_client: TunnelClient,
    buffer_sizes: BufferSizes,
}

impl ProxyHandler {
    fn spawn_tunnel(
        &self,
        mut upstream: DataStream,
        request: hyper::Request<hyper::body::Incoming>,
    ) {
        let buffer_sizes = self.buffer_sizes;

        tokio::spawn(async move {
            match hyper::upgrade::on(request).await.map(TokioIo::new) {
                Ok(mut upgraded) => {
                    if let Err(e) = tokio::io::copy_bidirectional_with_sizes(
                        &mut upgraded,
                        &mut upstream,
                        buffer_sizes.outgoing_buf,
                        buffer_sizes.incoming_buf,
                    )
                    .await
                    {
                        tracing::debug!("Data tunneling has failed: {}", e);
                    }
                }
                Err(e) => {
                    tracing::debug!("Tunnel upgrade has failed: {}", e);
                }
            }
        });
    }

    async fn handle(
        &self,
        request: hyper::Request<hyper::body::Incoming>,
    ) -> Result<hyper::Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        if !request.method().eq(&hyper::Method::CONNECT) {
            let response = build_full_response(
                http::StatusCode::NOT_IMPLEMENTED,
                "proxy only allows CONNECT request method",
            );

            return Ok(response);
        }

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
        let host = authority.host();

        let upstream = match self.tunnel_client.connect(host).await {
            Ok(upstream) => upstream,
            Err(e) => {
                tracing::warn!("Failed to connect to upstream {}: {}", host, e);

                let response = build_full_response(
                    http::StatusCode::SERVICE_UNAVAILABLE,
                    "failed to establish connection with upstream",
                );

                return Ok(response);
            }
        };
        self.spawn_tunnel(upstream, request);

        let response = build_empty_response(http::StatusCode::OK);

        Ok(response)
    }
}

pub struct Proxy {
    barrier: Barrier,
    handler: Arc<ProxyHandler>,
    port: u16,
}

impl Proxy {
    pub fn build(
        barrier: Barrier,
        tunnel_client: TunnelClient,
        port: u16,
        buffer_sizes: BufferSizes,
    ) -> Result<Self, ProxyError> {
        let handler = Arc::new(ProxyHandler {
            tunnel_client,
            buffer_sizes,
        });

        Ok(Self {
            barrier,
            handler,
            port,
        })
    }

    fn detatch(
        self,
        listener: TcpListener,
        mut exit_recv: tokio::sync::oneshot::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let (tcp_stream, addr) = tokio::select! {
                    result = listener.accept() => match result {
                        Ok(connection) => connection,
                        Err(e) => {
                            tracing::warn!("Failed while waiting for incoming connections: {}", e);

                            return;
                        },
                    },
                    _ = &mut exit_recv => return,
                };

                if let Some(wait_time) = self.barrier.jammed() {
                    tracing::warn!(
                        "Rate limiting during {:.2}s until before allowing new connections.",
                        wait_time.as_secs_f64()
                    );

                    continue;
                }

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
        })
    }

    pub async fn run(self) -> Result<(), ProxyError> {
        let addr = SocketAddr::from((LOCALHOST, self.port));

        let listener = TcpListener::bind(addr).await?;

        tracing::info!("Listening on port {}", self.port);

        let (exit_send, exit_recv) = tokio::sync::oneshot::channel();

        let mut proxy_handle = self.detatch(listener, exit_recv);

        tokio::select! {
            _ = &mut proxy_handle => (),
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Proxy is terminating...");

                let _ = exit_send.send(());
                let _ = proxy_handle.await;
            }
        }

        Ok(())
    }
}
