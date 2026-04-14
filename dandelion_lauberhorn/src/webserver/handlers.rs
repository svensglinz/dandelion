use machine_interface::function_driver::Metadata;
use machine_interface::memory_domain::read_only::ReadOnlyContext;
use machine_interface::{DataItem, DataSet, Position};
use machine_interface::composition::CompositionSet;
use std::path::PathBuf;
use std::io::Write;
use std::sync::{Arc};

use crate::runtime::Runtime;
use crate::webserver::schemas::{RegisterFunction, RegisterService};
use dandelion_server::DandelionBody;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::{Request, Response};
use machine_interface::function_driver::thread_utils::Engine;
use super::utils as webutils; 

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

pub async fn register_function<E: Engine>(
    req: Request<Incoming>,
    runtime: &Runtime<E>
) -> Result<Response<DandelionBody>, HandlerError> {

    let bytes = req.collect()
        .await
        .map_err(|e| HandlerError::BadRequest(format!("Failed to extract body from request: {}", e)))
        .unwrap()
        .to_bytes();

    let request_map: RegisterFunction =
        bson::from_slice(&bytes)
        .map_err(|e| HandlerError::BadRequest(format!("Failed to deserialize request: {}", e)))?;


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
        .map_err(|e| HandlerError::Internal(format!("Failed to create file for registering function: {}", e)))?;

        function_file
            .write_all(&request_map.binary)
            .map_err(|e| HandlerError::Internal(format!("Failed to write file with content for registering: {}", e)))?;
        path_buff.to_str().unwrap().to_string()
    };

    let engine_type = crate::utils::engine::get_engine_type(&request_map.engine_type)
        .map_err(|_| HandlerError::BadRequest(format!("Invalid engine type specified: {}", request_map.engine_type)))?;
    
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
        Ok(_) => Ok(webutils::make_ok("Function registered successfully")),
        Err(e) => Err(HandlerError::Internal(format!("Function registration failed: {}", e))),
    }
}


pub async fn register_service<E: Engine>(
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
        Ok(_) => Ok(webutils::make_ok("Service registered successfully")),
        Err(_) => Err(HandlerError::BadRequest("Service registration failed".into())),
    }
}

pub async fn serve_stats(_req: Request<Incoming>) -> Result<Response<DandelionBody>, HandlerError> {
    let archive = TRACING_ARCHIVE.get().unwrap();
    let response = archive.get_summary();
    
    archive.reset();
    Ok(webutils::make_ok(&response))
}
