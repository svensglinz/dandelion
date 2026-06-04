use dandelion_lauberhorn::webserver::schemas::{DandelionDeserializeResponse, DandelionRequest, InputItem, InputSet, RegisterChain};
use http::Response;
use std::vec;
mod common;
use common::{invoke_service, register_function};
use dandelion_lauberhorn::webserver::schemas::RegisterFunction;

use crate::common::{call_function, register_composition};

const MATMUL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../machine_interface/tests/data/test_elf_kvm_x86_64_matmul",
);

const MATMAC_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../machine_interface/tests/data/test_elf_kvm_x86_64_matmac",
);

fn get_matmul_reg_req() -> RegisterFunction {
    RegisterFunction {
        name: "matmul_x86".to_string(),
        context_size: 0x802_0000,
        engine_type: "Kvm".to_string(),
        local_path: "".to_string(),
        binary: std::fs::read(MATMUL_PATH)
            .expect("Failed to read test function binary"),
        input_sets: vec![(String::from(""), None)],
        output_sets: vec![String::from("")],
    }
}

fn get_matmac_reg_req() -> RegisterFunction 
{
    // 1. register function
    // function takes matrix A, B, C and computes A*B + C
    // matrix format (all i64): [num_rows, mat[0][0], mat[0][1], ..., mat[1][0], ...]
    RegisterFunction {
        name: "matmac_x86".to_string(),
        context_size: 0x992_0000, // what goes here ? 
        engine_type: "Kvm".to_string(),
        local_path: "".to_string(),
        binary: std::fs::read(MATMAC_PATH)
            .expect("Failed to read test function binary"),
        input_sets: vec![
            ("matrix_A".to_string(), None),
            ("matrix_B".to_string(), None),
            ("matrix_C".to_string(), None),
        ],
        output_sets: vec!["matrix_out".to_string()],
    }
}

#[test]
fn test_matmac_composition_sharding() {

let composition = r#"
    function matmac_x86 (mat_A, mat_B, mat_C) => (mat_out);
    function matmul_x86 (mat_in) => (mat_out);
    composition matmac_chain (mat_set_A, mat_set_B, mat_set_C) => (mat_set_out) {
        matmac_x86 (mat_A = keyed mat_set_A, mat_B = keyed mat_set_B, mat_C = keyed mat_set_C) 
        => (mat_set_tmp = mat_out) by mat_A left mat_B left mat_C;
        matmul_x86(mat_in = each mat_set_tmp) => (mat_set_out = mat_out);
    }
"#;

    // 1. register composition
    let chain_request = RegisterChain {
        composition: composition.to_string(),
    };

    // 2. register  functions needed in composition
    let reg_req = get_matmac_reg_req();
    let res = register_function("http://localhost:6000/register/function", &reg_req);
    assert_eq!(res.unwrap().status(), 200, "Failed to register function");

    let reg_req_matmul = get_matmul_reg_req();
    let res = register_function("http://localhost:6000/register/function", &reg_req_matmul);
    assert_eq!(res.unwrap().status(), 200, "Failed to register function");

    // 3. register composition
    let res = register_composition("http://localhost:6000/register/composition", &chain_request)
        .expect("Failed to register composition");

    // 2. build arguments for composition
    let matrix_a: Vec<u8> = vec![
        2i64,      // two rows
        1, 1,   // row 1
        1, 1    // row 2
    ].iter()
    .flat_map(|x| x.to_le_bytes())
    .collect();

    // 3. invoke composition 
    let mut input_items = Vec::new();
    for i in 0..3 {
        input_items.push(InputItem {
            identifier: String::from(""),
            key: i,
            data: matrix_a.clone()
        });
    }

    let input_set = InputSet{
        identifier: String::from(""),
        items: input_items.clone(),
    };
   
   let set_names = ["mat_set_A", "mat_set_B", "mat_set_C"];
    let request: Vec<InputSet> = set_names.iter().map(|&name| {
        InputSet {
            identifier: name.to_string(),
            items: input_items.clone(),
        }
    }).collect();

   let dan_request = DandelionRequest {
        name: "matmac_chain".into(),
        sets: request.clone(),
     };

     // Todo(@Sven): pass timeout as parameter
     let res = call_function("http://localhost:6000/cold/compute", &dan_request);
     println!("Response from call_function: {:?}", res);
     // let res =
     //     invoke_service("matmac_chain", 1, 1, 1, "
    // let res =
    //     invoke_service("matmac_chain", 1, 1, 1, "10.0.0.5", 12345, request);
    //     println!("Composition invocation response: {:?}", res);
}

#[test]
fn test_matmac() {

    // 1. register function
    // function takes matrix A, B, C and computes A*B + C
    // matrix format (all i64): [num_rows, mat[0][0], mat[0][1], ..., mat[1][0], ...]
    let req = RegisterFunction {
        name: "matmac_x86".to_string(),
        context_size: 0x802_0000, // what goes here ? 
        engine_type: "Kvm".to_string(),
        local_path: "".to_string(),
        binary: std::fs::read(MATMAC_PATH)
            .expect("Failed to read test function binary"),
        input_sets: vec![
            ("matrix_A".to_string(), None),
            ("matrix_B".to_string(), None),
            ("matrix_C".to_string(), None),
        ],
        output_sets: vec!["matrix_out".to_string()],
    };

    let res = register_function("http://localhost:6000/register/function", &req);
    assert_eq!(res.unwrap().status(), 200, "Failed to register function");
    
    // 2. invoke function
    let matrix_a: Vec<u8> = vec![
        2i64,      // two rows
        1, 1,   // row 1
        1, 1    // row 2
    ].iter()
    .flat_map(|x| x.to_le_bytes())
    .collect();

    let matrix_b: Vec<u8> = vec![
        2i64,      // two rows
        1, 2,   // row 1
        3, 4    // row 2
    ].iter()
    .flat_map(|x| x.to_le_bytes())
    .collect();

    let matrix_c: Vec<u8> = vec![
        2i64,      // two rows
        5, 6,   // row 1
        7, 8    // row 2
    ].iter()
    .flat_map(|x| x.to_le_bytes())
    .collect();

    let request = vec![InputSet {
            identifier: "matrix_A".to_string(),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: matrix_a,
            }],
        }, 
        InputSet {
            identifier: "matrix_B".to_string(),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: matrix_b,
            }],
        },
        InputSet {
            identifier: "matrix_C".to_string(),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: matrix_c,
            }],
        }];

        let request = DandelionRequest {
            name: "matmac_x86".into(),
            sets: request.clone(),
        };
        
        let res = call_function("http://localhost:6000/cold/compute", &request);
        println!("Response from call_function: {:?}", res);

    //  let res =
    //      invoke_service("matmac_x86", 1, 1, 1, "10.0.0.5", 12345, request)
    //          .expect("Failed to invoke service");
    // 
    // assert!(res.sets.len() == 1, "Expected exactly one output set");
    // let output_set = &res.sets[0];
    // assert_eq!(output_set.identifier, "matrix_out".to_string());
    // assert_eq!(output_set.items.len(), 1);
    // let output_data = output_set.items[0].data
    // .chunks(8)
    // .map(|chunk| i64::from_le_bytes(chunk.try_into().unwrap()))
    // .collect::<Vec<i64>>();
// 
    // let output_data_expected = vec![
    //     2i64,
    //     9, 12,   // row 1
    //     11, 14    // row 2
    // ];
    // assert_eq!(output_data, output_data_expected);
    // println!("{:?}", res);

}


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
        data.extend_from_slice(&i64::to_le_bytes(2));

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
fn invoke_matmul_dandelion_original() {
    let mut data = Vec::new();
    data.extend_from_slice(&i64::to_le_bytes(1));
        data.extend_from_slice(&i64::to_le_bytes(2));

    let mat_request = vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: data,
            }],
        }];

    // Todo(@Sven): pass timeout as parameter
    let res = call_function("http://localhost:6000/cold/compute", &DandelionRequest {
        name: "matmul_x86".into(),
        sets: mat_request.clone(),
    });

    println!("Response from call_function: {:?}", res);
    // let res =
    //     invoke_service("matmul_x86", 1, 1, 1, "10.0.0.5", 12345, mat_request)
    //         .expect("Failed to invoke service");

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
}


#[test]
fn invoke_matmul_chain_test() {

    // input data
    let mut data = Vec::new();
    data.extend_from_slice(&i64::to_le_bytes(1));
    data.extend_from_slice(&i64::to_le_bytes(2));

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