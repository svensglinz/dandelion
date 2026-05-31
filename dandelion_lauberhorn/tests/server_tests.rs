use dandelion_lauberhorn::webserver::schemas::{DandelionDeserializeResponse, InputItem, InputSet, RegisterChain};
use std::vec;
mod common;
use common::{invoke_service, register_function};
use dandelion_lauberhorn::webserver::schemas::RegisterFunction;

use crate::common::register_composition;

const MATMUL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../machine_interface/tests/data/test_elf_kvm_x86_64_matmul",
);

#[test]
fn register_matmul_composition() {

    let function_name = "matmul_x86";
    let version_string = "matmul_x86";

    let chain_name = format!("chain_{}", version_string);
    let chain_request = RegisterChain {
        composition: format!(
            r#"
            function {function} (InMats) => (OutMats);
            composition {chain} (CompInMats) => (CompOutMats) {{
                {function} (InMats = all CompInMats) => (InterMat = OutMats);
                {function} (InMats = all InterMat) => (CompOutMats = OutMats);
            }}
        "#,
            function = function_name,
            chain = chain_name,
        ),
    };

    let res = register_composition("http://localhost:6000/register/composition", &chain_request)
        .expect("Failed to register composition");
    
    println!("{:?}", res);
}



#[test]
fn register_matmul_x86() {

    let req = RegisterFunction {
        name: "matmul_x86".to_string(),
        context_size: 0x802_0000,
        engine_type: "Kvm".to_string(),
        local_path: "".to_string(),
        binary: std::fs::read(MATMUL_PATH)
            .expect("Failed to read test function binary"),
        input_sets: vec![(String::from(""), None)],
        output_sets: vec![String::from("")],
    };

    let res =
        register_function("http://localhost:6000/register/function", &req)
            .expect("Failed to register function");

    assert_eq!(res.status(), 200);
}

#[test]
fn invoke_service_test() {
    let mut data = Vec::new();
    data.extend_from_slice(&i64::to_le_bytes(1));
    data.extend_from_slice(&i64::to_le_bytes(1));

    let mat_request = vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: data,
            }],
        }];

    // Todo(@Sven): pass timeout as parameter
    let res =
        invoke_service("matmul_x86", 1, 1, 1, "10.0.0.5", 12345, mat_request)
            .expect("Failed to invoke service");

    let res_exp = DandelionDeserializeResponse {
        sets: vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: vec![1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
            }],
        }],
    };
    println!("Expected response: {:?}", res_exp);
    println!("Actual response: {:?}", res);
}

#[test]
fn invoke_malmul_chain_test() {

    // input data
    let mut data = Vec::new();
    data.extend_from_slice(&i64::to_le_bytes(1));
    data.extend_from_slice(&i64::to_le_bytes(1));

    let mat_data = vec![InputSet {
        identifier: String::from(""),
        items: vec![InputItem {
            identifier: String::from(""),
            key: 0,
            data: data,
        }],
    }];

    let res = 
        invoke_service("chain_matmul_x86", 1, 1, 1, "10.0.0.5", 12345, mat_data)
        .expect("Failed to invoke composition");

    println!("Actual response: {:?}", res);

}