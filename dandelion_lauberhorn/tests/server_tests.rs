use std::vec;

use dandelion_server::{DandelionRequest,};
use reqwest::blocking::{Client, Response};
use dandelion_lauberhorn::webserver::schemas::RegisterFunction;
use bson::ser::to_vec;

fn register_function(url: &str, obj: &RegisterFunction) -> Result<Response, ()> {
    let client = Client::new();

    let body = to_vec(obj).expect("BSON serialization failed");

    let res = client
    .post(url)
    .header("content-type", "application/octet-stream")
    .body(body)
    .send()
    .expect("Failed to send request");

    Ok(res)
}

#[test]
fn register_function_test() {

    let req = RegisterFunction {
        name: "test_func".to_string(),
        context_size: 1024,
        engine_type: "Process".to_string(),
        local_path: "".to_string(),
        binary: vec![],
        input_sets: vec![],
        output_sets: vec![],
    };

    let res = register_function("http://localhost:8080/register/function", &req)
        .expect("Failed to register function");

    println!("Response: {:?}", res.bytes());
}