
/// create new http responses
/// 
use http::{Response}; 
use http::StatusCode; 

use dandelion_server::DandelionBody;

// why dandelionBody at all ? 
// TODO: use ResponseBuilder
pub fn make_bad_request(msg: &str) -> Response<DandelionBody> {
    let mut resp =  
    Response::new(DandelionBody::from_vec(msg.as_bytes().to_vec()));
    *resp.status_mut() = StatusCode::BAD_REQUEST;
    resp
}

pub fn make_internal_error(msg: &str) -> Response<DandelionBody> {
    let mut resp = 
    Response::new(DandelionBody::from_vec(msg.as_bytes().to_vec()));
    *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    resp 
}

pub fn make_ok(msg: &str) -> Response<DandelionBody> {
    Response::new(DandelionBody::from_vec(msg.as_bytes().to_vec()))
}
