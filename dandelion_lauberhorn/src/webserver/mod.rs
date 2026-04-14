mod handlers;
mod router;
pub mod schemas;
pub mod utils;

use crate::runtime::Runtime;
use hyper::service::service_fn;
use log::{error, info};
use machine_interface::function_driver::thread_utils::Engine;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal::unix::SignalKind;

use router::route;

// ---------------------------------------------------------------------------
// Webserver loop which handles incoming requests
// ---------------------------------------------------------------------------

pub async fn service_loop<E: Engine + 'static>(runtime: Arc<Runtime<E>>, port: u16) {
    let addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], port));

    // bind TCP listener
    let Ok(listener) = TcpListener::bind(addr).await else {
        error!("Failed to bind to address {}:{}", addr.ip(), addr.port());
        return;
    };
    info!("HTTP server listening on {}", addr);

    // signal handlers for graceful shutdown
    let mut sigterm_stream =
        tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
    let mut sigint_stream = tokio::signal::unix::signal(SignalKind::interrupt()).unwrap();
    let mut sigquit_stream = tokio::signal::unix::signal(SignalKind::quit()).unwrap();

    loop {
        tokio::select! {
            connection_pair = listener.accept() => {
                let (stream, _) = connection_pair.unwrap();
                let rt = runtime.clone();
                let io = hyper_util::rt::TokioIo::new(stream);
                tokio::task::spawn(async move {
                    let service_rt = rt.clone();
                    if let Err(err) = hyper_util::server::conn::auto::Builder::new(
                        hyper_util::rt::TokioExecutor::new()
                    )
                    .serve_connection_with_upgrades(
                        io,
                        service_fn(|req| route(req, service_rt.clone())),
                    )
                    .await
                    {
                        error!("Request serving failed with error: {:?}", err);
                    }
                });
            }
            _ = sigterm_stream.recv() => {
                info!("Received SIGTERM, shutting down...");
                return;
            },
            _ = sigint_stream.recv() => {
                info!("Received SIGINT, shutting down...");
                return;
            },
            _ = sigquit_stream.recv() => {
                info!("Received SIGQUIT, shutting down...");
                return;
            },
        }
    }
}
