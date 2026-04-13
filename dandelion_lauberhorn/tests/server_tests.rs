use std::vec;

mod common;
use common::{register_function, register_service, invoke_service};
use dandelion_lauberhorn::webserver::schemas::{RegisterFunction, RegisterService};

const MATMUL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../machine_interface/tests/data/test_elf_mmu_x86_64_matmul",
);

#[test]
fn register_function_test() {

    let req = RegisterFunction {
        name: "test_func".to_string(),
        context_size: 0x802_0000,
        engine_type: "Process".to_string(),
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
        function_id: "test_func".to_string(),
        prog_num: 1,
        prog_ver: 1,
        proc_num: 1,
        listen_port: 5555, // do we need to check if this is already taken by another process upon registration ? bc. this binding is not registered
        // in the kernel with lauberhorn !
    };

    let res = register_service("http://localhost:6000/register/service", &req)
        .expect("Failed to register service");

    println!("Response: {:?}", res.bytes());
}

#[test]
fn invoke_service_test() {
        // Prepare input data: two i64 values (e.g., matrix size and checksum)
    let mut data = Vec::new();
    data.extend_from_slice(&i64::to_le_bytes(1)); // matrix size or value
    data.extend_from_slice(&i64::to_le_bytes(1)); // checksum or another value

    invoke_service("test_func", 1, 1, 1, "10.0.0.5", 5555, Some(data))
        .expect("Failed to invoke service");
}