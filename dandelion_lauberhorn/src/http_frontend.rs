use std::convert::Infallible;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use dandelion_commons::records::Archive;
use dandelion_lauberhorn::runtime::Runtime;
use dandelion_server::DandelionBody;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use log::{error, info};
use machine_interface::composition::CompositionSet;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::EngineType;
use machine_interface::memory_domain::read_only::ReadOnlyContext;
use machine_interface::{DataItem, DataSet, Position};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::signal::unix::SignalKind;
use crate::http_schemas::{RegisterFunction};
use crate::http_response::*;

pub const FUNCTION_FOLDER_PATH: &str = "/tmp/dandelion_server";

pub static TRACING_ARCHIVE: OnceLock<Archive> = OnceLock::new();

enum HandlerError {
    BadRequest(String),
    Internal(String),
}

impl HandlerError {
    fn to_response(&self) -> Response<DandelionBody> {
        match self {
            HandlerError::BadRequest(msg) => http_response::make_bad_request(msg),
            HandlerError::Internal(msg) => http_response::butmake_internal_error(msg),
        }
    }
}

// ---------------------------------------------------------------------------
// Endpoint handlers
// ---------------------------------------------------------------------------

async fn register_function<E: Engine>(
    req: Request<Incoming>,
    runtime: &Runtime<E>
) -> Result<Response<DandelionBody>, HandlerError> {

    let bytes = match req.collect().await {
        Ok(body) => body.to_bytes(),
        Err(e) => {
            return Err(HandlerError::BadRequest("Failed to extract body from request".into()));
        }
    };

    let request_map: RegisterFunction =
        bson::from_slice(&bytes).expect("Should be able to deserialize request");


    let path_string = if !request_map.local_path.is_empty() {
        if let Err(err) = std::fs::File::open(&request_map.local_path) {
            let err_message = format!(
                "Tried to register function with local path, but failed to open file with error {}",
                err
            );
            return Err(HandlerError::Internal(err_message));
        };
        request_map.local_path
    } else {
        std::fs::create_dir_all(FUNCTION_FOLDER_PATH).unwrap();
        let mut path_buff = PathBuf::from(FUNCTION_FOLDER_PATH);
        path_buff.push(request_map.name.clone());
        let mut function_file = std::fs::File::create(path_buff.clone())
            .expect("Failed to create file for registering function");
        function_file
            .write_all(&request_map.binary)
            .expect("Failed to write file with content for registering");
        path_buff.to_str().unwrap().to_string()
    };

    let engine_type = match request_map.engine_type.as_str() {
        #[cfg(feature = "mmu")]
        "Process" => EngineType::Process,
        #[cfg(feature = "kvm")]
        "Kvm" => EngineType::Kvm,
        #[cfg(feature = "cheri")]
        "Cheri" => EngineType::Cheri,
        unknown: => return Err(HandlerError::BadRequest(format!("Unknown engine type {}", unknown))), 
    };
    
    let input_sets = request_map
        .input_sets
        .into_iter()
        .map(|(name, data)| {
            if let Some(static_data) = data {
                let data_contexts = static_data
                    .into_iter()
                    .map(|(item_name, data_vec)| {
                        let item_size = data_vec.len();
                        let mut new_context =
                            ReadOnlyContext::new(data_vec.into_boxed_slice()).unwrap();
                        new_context.content.push(Some(DataSet {
                            ident: name.clone(),
                            buffers: vec![DataItem {
                                ident: item_name,
                                data: Position {
                                    offset: 0,
                                    size: item_size,
                                },
                                key: 0,
                            }],
                        }));
                        Arc::new(new_context)
                    })
                    .collect();
                let composition_set = CompositionSet::from((0, data_contexts));
                (name, Some(composition_set))
            } else {
                (name, None)
            }
        })
        .collect();

    let metadata = Metadata {
        input_sets,
        output_sets: request_map.output_sets,
    };

    match runtime.register_function(
        request_map.name, engine_type, request_map.context_size as usize,
        path_string, metadata,
    ) {
        Ok(_) => webserver::utils::make_ok("Function registered successfully"),
        Err(_) => Err(HandlerError::Internal("Function registration failed".into())),
    }
}

#[derive(Debug, Deserialize)]
struct RegisterService {
    function_id: String,
    prog_num: u32,
    prog_ver: u32,
    proc_num: u32,
    listen_port: u16,
}

async fn register_service<E: Engine>(
    req: Request<Incoming>,
    runtime: &Runtime<E>,
) -> Result<Response<DandelionBody>, HandlerError> {
    let bytes =  req
        .collect()
        .await
        .map_err(|e| HandlerError::BadRequest("Failed to extract body from request".into()))?
        .to_bytes();
    let request_map: RegisterService =
        bson::from_slice(&bytes)
        .map_err(|e| HandlerError::BadRequest("Failed to deserialize request".into()))?;
    match runtime.register_service(
        Arc::new(request_map.function_id), request_map.prog_num,
        request_map.prog_ver, request_map.proc_num, request_map.listen_port,
    ) {
        Ok(_) => webserver::utils::make_ok("Service registered successfully"),
        Err(_) => Err(HandlerError::BadRequest("Service registration failed".into())),
    }
}

async fn serve_stats(_req: Request<Incoming>) -> Result<Response<DandelionBody>, Infallible> {
    let archive = TRACING_ARCHIVE.get().unwrap();
    let response = Response::new(DandelionBody::from_vec(
        archive.get_summary().into_bytes(),
    ));
    archive.reset();
    Ok::<_, Infallible>(response)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

async fn service<E: Engine>(
    req: Request<Incoming>,
    runtime: Arc<Runtime<E>>,
) -> Result<Response<DandelionBody>, HandlerError> {

    info!("Incoming HTTP request: {} {}", req.method(), req.uri().path());

    let result = match req.uri().path() {
        "/register/function" => register_function(req, &runtime).await,
        "/register/service" => register_service(req, &runtime).await,
        "/stats" => serve_stats(req).await,
        _ => Ok(not_found()),
    };

    match result {
        Ok(resp) => Ok(resp),
        Err(e) => Ok(e.to_response())
    }
}

fn not_found() -> Response<DandelionBody> {
    Response::new(DandelionBody::from_vec(b"Not found".to_vec()))
}

// ---------------------------------------------------------------------------
// Accept loop
// ---------------------------------------------------------------------------

/// Accept connections and serve them with signal-based graceful shutdown.
pub async fn service_loop<E: Engine + 'static>(runtime: Arc<Runtime<E>>, port: u16) {
    let addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();

    // signal handlers for graceful shutdown
    let mut sigterm_stream = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
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
                        service_fn(|req| service(req, service_rt.clone())),
                    )
                    .await
                    {
                        error!("Request serving failed with error: {:?}", err);
                    }
                });
            }
            _ = sigterm_stream.recv() => return,
            _ = sigint_stream.recv() => return,
            _ = sigquit_stream.recv() => return,
        }
    }
}