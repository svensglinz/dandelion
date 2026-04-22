use std::vec;
use dandelion_server::DandelionRequest;
use dandelion_server::{InputSet, InputItem};
mod common;
use common::{register_function, register_service, invoke_service};
use dandelion_lauberhorn::webserver::schemas::{RegisterFunction, RegisterService};

const MATMUL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../machine_interface/tests/data/test_elf_kvm_x86_64_matmul",
);

// TODO: 
// CHANGE LAUBerhorn lib to accept connections dynamically
// maybe use portmapper to give port back automatically ? 
// regiter ever only 1 endpoint --> multiplex via funtion_name which will become a field in XDR requeest
// ie. xdr requet will consist of funcion_name: String, data: BJSON-Blob

#[test]
fn register_function_test() {

    let req = RegisterFunction {
        name: "test_func2".to_string(),
        context_size: 0x802_0000,
        engine_type: "Kvm".to_string(),
        local_path: "".to_string(),
        binary: std::fs::read(MATMUL_PATH).expect("Failed to read test function binary"),
        input_sets: vec![(String::from(""), None)],
        output_sets: vec![String::from("")],
    };

    let res = register_function("http://localhost:6000/register/function", &req)
        .expect("Failed to register function");

    println!("Response: {:?}", res.bytes());
}


#[test]
fn register_service_test() {
    let req = RegisterService {
        function_id: "test_func2".to_string(),
        prog_num: 1,
        prog_ver: 1,
        proc_num: 1,
        listen_port: 5555, // do we need to check if this is already taken by another process upon registration ? bc. this binding is not registered
        // in the kernel with lauberhorn
    };

    let res = register_service("http://localhost:6000/register/service", &req)
        .expect("Failed to register service");

    println!("Response: {:?}", res.bytes());
}

#[test]
fn invoke_service_test() {

    let mut data = Vec::new();
    data.extend_from_slice(&i64::to_le_bytes(1));
    data.extend_from_slice(&i64::to_le_bytes(1));

    let mat_request = DandelionRequest {
        name: "test_func1".to_string(),
        sets: vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: &data
            }]
        }]
    };

    invoke_service(
        "test_func1",
        1, 1, 1,
        "10.0.0.5", 5555,
        &mat_request
    )
        .expect("Failed to invoke service");
}