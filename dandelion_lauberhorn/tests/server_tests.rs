use std::vec;

use dandelion_server::{DandelionRequest,};
use reqwest::blocking::{Client, Response};
use dandelion_lauberhorn::webserver::schemas::{RegisterFunction, RegisterService};
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

fn register_service(url: &str, obj: &RegisterService) -> Result<Response, ()> {
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

#[test]
fn register_service_test() {
    let req = RegisterService {
        function_id: "test_func".to_string(),
        prog_num: 1,
        prog_ver: 1,
        proc_num: 1,
        listen_port: 6000, // do we need to check if this is already taken by another process upon registration ? bc. this binding is not registered
        // in the kernel with lauberhorn !
    };

    let res = register_service("http://localhost:8080/register/service", &req)
        .expect("Failed to register service");

    println!("Response: {:?}", res.bytes());
}