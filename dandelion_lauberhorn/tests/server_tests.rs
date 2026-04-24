use std::vec;
use dandelion_lauberhorn::{lauberhorn::types::DandelionArgs, webserver::schemas::{DandelionDeserializeResponse, InputItem, InputSet}};
mod common;
use common::{register_function, invoke_service};
use dandelion_lauberhorn::webserver::schemas::{RegisterFunction};

const MATMUL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../machine_interface/tests/data/test_elf_kvm_x86_64_matmul",
);


#[test]
fn register_matmul_x86() {

    let req = RegisterFunction {
        name: "matmul_x86".to_string(),
        context_size: 0x802_0000,
        engine_type: "Kvm".to_string(),
        local_path: "".to_string(),
        binary: std::fs::read(MATMUL_PATH).expect("Failed to read test function binary"),
        input_sets: vec![(String::from(""), None)],
        output_sets: vec![String::from("")],
    };

    let res = register_function("http://localhost:6000/register/function", &req)
        .expect("Failed to register function");

    assert_eq!(res.status(), 200);
}

#[test]
fn invoke_service_test() {
    
    let mut data = Vec::new();
    data.extend_from_slice(&i64::to_le_bytes(1));
    data.extend_from_slice(&i64::to_le_bytes(1));

    // Todo(@Sven): remove name from DandelionRequest
    let mat_request = DandelionArgs {
        sets: vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: data
            }]
        }]
    };

    // Todo(@Sven): pass timeout as parameter
    let res =invoke_service(
        "matmul_x86",
        1, 1, 1,
        "10.0.0.5", 11111,
        &mat_request
    )
        .expect("Failed to invoke service");

    let res_exp = DandelionDeserializeResponse {
        sets: vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: vec![1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
            }]
        }]
    };

    // extract RPC header
    let res_exp_bytes = bson::to_vec(&res_exp)
    .expect("Failed to serialize expected response");

    println!("Expected response bytes: {:?}", res_exp_bytes);
    println!("Actual response bytes: {:?}", res);
    assert_eq!(res_exp_bytes, res);
}