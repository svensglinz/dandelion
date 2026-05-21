use bytes::Bytes;
use dandelion_commons::{DandelionError, DandelionResult};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use super::utils as webutils;
use crate::runtime::Runtime;
use crate::webserver::schemas::{
    RegisterChain, RegisterFunction, RegisterService,
};
use dandelion_server::DandelionBody;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::{Request, Response};
use machine_interface::function_driver::thread_utils::Engine;

use crate::system::{FUNCTION_FOLDER_PATH, TRACING_ARCHIVE};

// ---------------------------------------------------------------------------
// Handle Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum HandlerError {
    BadRequest(String),
    Internal(String),
}

impl HandlerError {
    pub fn to_response(&self) -> Response<DandelionBody> {
        match self {
            HandlerError::BadRequest(msg) => webutils::make_bad_request(msg),
            HandlerError::Internal(msg) => webutils::make_internal_error(msg),
        }
    }
}

// ---------------------------------------------------------------------------
// Endpoint handlers
// ---------------------------------------------------------------------------

fn save_function_binary(
    name: &str,
    binary: &Vec<u8>,
) -> DandelionResult<String> {
    std::fs::create_dir_all(FUNCTION_FOLDER_PATH).unwrap();
    let mut path_buff = PathBuf::from(FUNCTION_FOLDER_PATH);
    path_buff.push(name);

    let mut function_file = std::fs::File::create(path_buff.clone())
        .map_err(|_| DandelionError::FileError)?;

    function_file.write_all(binary).map_err(|_| DandelionError::FileError)?;

    Ok(path_buff.to_str().unwrap().to_string())
}

// helper function to extract bytes from request body
async fn collect_bytes(req: Request<Incoming>) -> Result<Bytes, HandlerError> {
    // 1. Get the body out of the request (consumes req)
    let body = req.into_body();

    let bytes = body
        .collect()
        .await
        .map_err(|_| {
            HandlerError::BadRequest(
                "Failed to extract body from request".into(),
            )
        })?
        .to_bytes();
    Ok(bytes)
}

/// Handler to register a new composition with the runtime
/// 
/// Deserializes the incoming request to `RegisterChain`
/// and then registers the composition with the runtime
pub async fn register_composition<E: Engine>(
    req: Request<Incoming>,
    runtime: &Runtime<E>,
) -> Result<Response<DandelionBody>, HandlerError> {
    let bytes = collect_bytes(req).await?;

    let request_map: RegisterChain = bson::from_slice(&bytes).map_err(|e| {
        HandlerError::BadRequest(format!(
            "Failed to deserialize request: {}",
            e
        ))
    })?;

    runtime.register_composition(&request_map.composition.as_str()).map_err(
        |e| {
            HandlerError::Internal(format!(
                "Failed to register composition: {}",
                e
            ))
        },
    )?;
    return Ok(webutils::make_ok("Composition registered successfully"));
}

/// Handler to register a new function with the runtime
/// 
/// Deserializes the incoming request to `RegisterFunction`
/// and then registers the function with the runtime
pub async fn register_function<E: Engine>(
    req: Request<Incoming>,
    runtime: &Runtime<E>,
) -> Result<Response<DandelionBody>, HandlerError> {
    let bytes = collect_bytes(req).await?;

    let request_map: RegisterFunction =
        bson::from_slice(&bytes).map_err(|e| {
            HandlerError::BadRequest(format!(
                "Failed to deserialize request: {}",
                e
            ))
        })?;

    let path_string = if !request_map.local_path.is_empty() {
        if let Err(err) = std::fs::File::open(&request_map.local_path) {
            return Err(HandlerError::BadRequest(format!(
                "Failed to open local path for function binary: {}",
                err
            )));
        }
        request_map.local_path.clone()
    } else {
        save_function_binary(&request_map.name, &request_map.binary).map_err(
            |e| {
                HandlerError::Internal(format!(
                    "Failed to save function binary: {}",
                    e
                ))
            },
        )?
    };

    let engine_type =
        crate::utils::engine::get_engine_type(&request_map.engine_type)
            .map_err(|_| {
                HandlerError::BadRequest(format!(
                    "Invalid engine type specified: {}",
                    request_map.engine_type
                ))
            })?;

    let ctx_size = request_map.context_size as usize;
    let function_name = request_map.name.clone();
    let metadata = request_map.into_metadata();

    match runtime.register_function(
        function_name,
        engine_type,
        ctx_size,
        path_string,
        metadata,
    ) {
        Ok(_) => Ok(webutils::make_ok("Function registered successfully")),
        Err(e) => Err(HandlerError::Internal(format!(
            "Function registration failed: {}",
            e
        ))),
    }
}

// TODO(@Sven): NOT USED ANYMORE (since we register  all functions under the same handler !)
// pub async fn register_service<E: Engine>(
//     req: Request<Incoming>,
//     runtime: &Runtime<E>,
// ) -> Result<Response<DandelionBody>, HandlerError> {
//     let bytes = collect_bytes(req).await?;
// 
//     let request_map: RegisterService =
//         bson::from_slice(&bytes).map_err(|_| {
//             HandlerError::BadRequest("Failed to deserialize request".into())
//         })?;
//     
//     match runtime.register_service(
//         Arc::new(request_map.function_id),
//         request_map.prog_num,
//         request_map.prog_ver,
//         request_map.proc_num,
//         request_map.listen_port,
//     ) {
//         Ok(_) => Ok(webutils::make_ok("Service registered successfully")),
//         Err(_) => {
//             Err(HandlerError::BadRequest("Service registration failed".into()))
//         }
//     }
// }

pub async fn serve_stats(
    _req: Request<Incoming>,
) -> Result<Response<DandelionBody>, HandlerError> {
    let archive = TRACING_ARCHIVE.get().unwrap();
    let response = archive.get_summary();

    archive.reset();
    Ok(webutils::make_ok(&response))
}
