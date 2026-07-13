use dandelion_lauberhorn::webserver::schemas::{DandelionDeserializeResponse, InputItem, InputSet, RegisterChain};
use std::vec;
mod common;
use common::{deregister, invoke_service, register_function};
use dandelion_lauberhorn::webserver::schemas::RegisterFunction;
use crate::common::register_composition;

struct TestConfig {
    binary_path: String,
    binary_arch: String,
    lauberhorn_ip: String,
    lauberhorn_port: u16,
    dandelion_server: String,
    dandelion_port: u16,
    dandelion_isol_type: String,
    lauberhorn_prog_num: u32, 
    lauberhorn_prog_ver: u32,
    lauberhorn_proc_num: u32,
}

impl TestConfig {
    fn from_env() -> Self {
        let binary_path = std::env::var("DANDELION_TEST_BINARY_PATH")
            .expect("DANDELION_TEST_BINARY_PATH must be set");
        let binary_arch = std::env::var("DANDELION_TEST_BINARY_ARCH")
            .expect("DANDELION_TEST_BINARY_ARCH must be set");
        let lauberhorn_ip = std::env::var("LAUBERHORN_IP")
            .expect("LAUBERHORN_IP must be set");
        let dandelion_server = std::env::var("DANDELION_SERVER")
            .expect("DANDELION_SERVER must be set");
        let lauberhorn_port = std::env::var("LAUBERHORN_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12345);
        let dandelion_port = std::env::var("DANDELION_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(6000);
        let dandelion_isol_type = std::env::var("DANDELION_ISOL_TYPE")
            .expect("DANDELION_ISOL_TYPE must be set");
        let lauberhorn_prog_num = std::env::var("LAUBERHORN_PROG_NUM")
            .expect("LAUBERHORN_PROG_NUM must be set")
            .parse()
            .expect("LAUBERHORN_PROG_NUM must be a valid number");
        let lauberhorn_prog_ver = std::env::var("LAUBERHORN_PROG_VER")
            .expect("LAUBERHORN_PROG_VER must be set")
            .parse()
            .expect("LAUBERHORN_PROG_VER must be a valid number");
        let lauberhorn_proc_num = std::env::var("LAUBERHORN_PROC_NUM")
            .expect("LAUBERHORN_PROC_NUM must be set")
            .parse()
            .expect("LAUBERHORN_PROC_NUM must be a valid number");
        
        TestConfig {
            binary_path,
            binary_arch,
            lauberhorn_ip,
            lauberhorn_port,
            dandelion_server,
            dandelion_port,
            dandelion_isol_type,
            lauberhorn_prog_num, 
            lauberhorn_prog_ver,
            lauberhorn_proc_num,
        }
    }

    fn server_url(&self, path: &str) -> String {
        format!("http://{}:{}{}", self.dandelion_server, self.dandelion_port, path)
    }

    fn test_binary(&self, base_name: &str) -> String {
        format!("{}/test_elf_{}_{}_{}", self.binary_path, self.dandelion_isol_type, self.binary_arch, base_name)
    }
}

fn get_matmul_reg_req(config: &TestConfig) -> RegisterFunction {
    let path = config.test_binary("matmul");
    RegisterFunction {
        name: "matmul".to_string(),
        context_size: 0x802_0000,
        engine_type: get_engine_name(),
        local_path: "".to_string(),
        binary: std::fs::read(path)
            .expect("Failed to read test function binary"),
        input_sets: vec![(String::from(""), None)],
        output_sets: vec![String::from("")],
    }
}

// function takes matrix A, B, C and computes A*B + C
// matrix format (all i64): [num_rows, mat[0][0], mat[0][1], ..., mat[1][0], ...]
fn get_matmac_reg_req(config: &TestConfig) -> RegisterFunction {
    let path = config.test_binary("matmac");
    RegisterFunction {
        name: "matmac".to_string(),
        context_size: 0x992_0000,
        engine_type: get_engine_name(),
        local_path: "".to_string(),
        binary: std::fs::read(path)
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
fn test_matmul_matmac_composition() {
    let config = TestConfig::from_env();

    // just something random to test compositions
    let composition = r#"
    function matmul (mat_in) => (matmul_out);
    function matmac (mat_A, mat_B, mat_C) => (matmac_out);
    composition graph (matA, matB, matC, matD) => (matResult) {
        matmul(mat_in = all matA) => (mat1_out = matmul_out);
        matmul(mat_in = all matB) => (mat2_out = matmul_out);
        matmac(mat_A = all mat1_out, mat_B = all mat2_out, mat_C = all matC) => (mat3_out = matmac_out);
        matmac(mat_A = all mat3_out, mat_B = all matD, mat_C = all mat2_out) => (matResult = matmac_out);
    }
"#;
        // 1. register functions
        let reg_req = get_matmac_reg_req(&config);
        let res = register_function(&config.server_url("/register/function"), &reg_req);
        assert_eq!(res.unwrap().status(), 200, "Failed to register function");

        let reg_req_matmul = get_matmul_reg_req(&config);
        let res = register_function(&config.server_url("/register/function"), &reg_req_matmul);
        assert_eq!(res.unwrap().status(), 200, "Failed to register function");

        // 3. register composition
        let chain_request = RegisterChain {
            composition: composition.to_string(),
        };

        let _ = register_composition(&config.server_url("/register/composition"), &chain_request)
            .expect("Failed to register composition");

         // 2. build arguments for composition
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

        let matrix_d: Vec<u8> = vec![
            2i64,      // two rows
            1, -1,   // row 1
            -1, 1    // row 2
        ].iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();

    let matrices = vec![matrix_a, matrix_b, matrix_c, matrix_d];
    let mut input_sets = Vec::new();
    for i in 0..4 {
        input_sets.push({
            InputSet {
                identifier: String::from(""),
                items: vec![InputItem {
                    identifier: String::from(""),
                    key: 0,
                    data: matrices[i as usize].clone(),
                }],
            }
        });
    }

    let res =
    invoke_service(
        "graph", config.lauberhorn_prog_num,
        config.lauberhorn_prog_ver, config.lauberhorn_proc_num,
        &config.lauberhorn_ip, config.lauberhorn_port,
        input_sets
    )
    .expect("invoke service failed");

    println!("Response from call_function: {:?}", res);

    let d = res.sets[0].items[0].data
    .chunks(8)
    .map(|chunk| i64::from_le_bytes(chunk.try_into().unwrap()))
    .collect::<Vec<i64>>();

    println!("Output data: {:?}", d);
    assert_eq!(d, vec![2, -36, 52, -30, 66]);

    let dereg_url = config.server_url("/deregister");
    assert_eq!(deregister(&dereg_url, "graph").unwrap().status(), 200, "Failed to deregister composition");
    assert_eq!(deregister(&dereg_url, "matmac").unwrap().status(), 200, "Failed to deregister function");
    assert_eq!(deregister(&dereg_url, "matmul").unwrap().status(), 200, "Failed to deregister function");
}


#[test]
fn test_matmac_composition_sharding() {
    let config = TestConfig::from_env();
    let composition = r#"
    function matmac (mat_A, mat_B, mat_C) => (mat_out);
    function matmul (mat_in) => (mat_out);
    composition matmac_chain (mat_set_A, mat_set_B, mat_set_C) => (mat_set_out) {
        matmac (mat_A = keyed mat_set_A, mat_B = keyed mat_set_B, mat_C = keyed mat_set_C) 
        => (mat_set_tmp = mat_out) by mat_A inner mat_B inner mat_C;
        matmul(mat_in = each mat_set_tmp) => (mat_set_out = mat_out);

    }
"#;
    // 1. register composition
    let chain_request = RegisterChain {
        composition: composition.to_string(),
    };

    // 2. register  functions needed in composition
    let reg_req = get_matmac_reg_req(&config);
    let res = register_function(&config.server_url("/register/function"), &reg_req);
    assert_eq!(res.unwrap().status(), 200, "Failed to register function");

    let reg_req_matmul = get_matmul_reg_req(&config);
    let res = register_function(&config.server_url("/register/function"), &reg_req_matmul);
    assert_eq!(res.unwrap().status(), 200, "Failed to register function");

    let res = register_composition(&config.server_url("/register/composition"), &chain_request)
        .expect("Failed to register composition");

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

    let matrices = vec![matrix_a, matrix_b, matrix_c];

    let mut input_items = Vec::new();
    for i in 0..3 {
        input_items.push(InputItem {
            identifier: String::from(""),
            key: i,
            data: matrices[i as usize].clone()
        });
    }
   
   let set_names = ["mat_set_A", "mat_set_B", "mat_set_C"];
    let request: Vec<InputSet> = set_names.iter().map(|&name| {
        InputSet {
            identifier: name.to_string(),
            items: input_items.clone(),
        }
    }).collect();

    println!("Response from call_function: {:?}", res);

    let res =
        invoke_service("matmac_chain", config.lauberhorn_prog_num, config.lauberhorn_prog_ver, config.lauberhorn_proc_num, &config.lauberhorn_ip, config.lauberhorn_port, request)
        .expect("invoke service failed");
        println!("Composition invocation response: {:?}", res);

    assert!(res.sets.len() == 1, "Expected one output set");
    assert!(res.sets[0].items.len() == 3, "Expected three output items in the output set");

    let expected_options = vec![
            vec![2, 18, 18, 18, 18],   // row 1
            vec![2, 208, 456, 456, 1000],   // row 2
            vec![2, 12240, 16632, 16632, 22600],   // row 3
    ];

    for set in &res.sets[0].items {
        let output_data = set.data
        .chunks(8)
        .map(|chunk| i64::from_le_bytes(chunk.try_into().unwrap()))
        .collect::<Vec<i64>>();
        println!("Output data: {:?}", output_data);
        assert!(expected_options.contains(&output_data), "Unexpected output data");
    }

    let dereg_url = config.server_url("/deregister");
    assert_eq!(deregister(&dereg_url, "matmac_chain").unwrap().status(), 200, "Failed to deregister composition");
    assert_eq!(deregister(&dereg_url, "matmac").unwrap().status(), 200, "Failed to deregister function");
    assert_eq!(deregister(&dereg_url, "matmul").unwrap().status(), 200, "Failed to deregister function");
}


#[test]
fn test_matmac() {
    let config = TestConfig::from_env();

    // register function
    let req = get_matmac_reg_req(&config);
    let res = register_function(&config.server_url("/register/function"), &req);
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

    let res =
        invoke_service("matmac", config.lauberhorn_prog_num, config.lauberhorn_prog_ver, config.lauberhorn_proc_num, &config.lauberhorn_ip, config.lauberhorn_port, request)
            .expect("Failed to invoke service");
    
    // also test reply composition
    assert!(res.sets.len() == 1, "Expected exactly one output set");
    let output_set = &res.sets[0];
    assert_eq!(output_set.identifier, "matrix_out".to_string());
    assert_eq!(output_set.items.len(), 1);
    let output_data = output_set.items[0].data
        .chunks(8)
        .map(|chunk| i64::from_le_bytes(chunk.try_into().unwrap()))
        .collect::<Vec<i64>>();

    let output_data_expected = vec![
        2i64,
        9, 12,   // row 1
        11, 14    // row 2
    ];

    // check for data equality
    assert_eq!(output_data, output_data_expected);
    
    let dereg_url = config.server_url("/deregister");
    assert_eq!(deregister(&dereg_url, "matmac").unwrap().status(), 200, "Failed to deregister function");
}


// test invoking composition inside a composition
#[test]
fn test_chained_composition() {
    let config = TestConfig::from_env();

    let function_name = "matmul";
    let reg_req_matmul = get_matmul_reg_req(&config);
    let res = register_function(&config.server_url("/register/function"), &reg_req_matmul);
    assert_eq!(res.unwrap().status(), 200, "Failed to register function");

    let chain_name = "matmul_chain";
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

    let chain_name_2 = "matmul_chain_composition";
    let chain_request2 = RegisterChain {
        composition: format!(
            r#"
            function {chain_name} (CompInMats) => (CompOutMats);
            composition {chain} (Comp2InMats) => (Comp2OutMats) {{
                {chain_name} (CompInMats = all Comp2InMats) => (InterMat = CompOutMats);
                {chain_name} (CompInMats = all InterMat) => (Comp2OutMats = CompOutMats);
            }}
        "#,
            chain_name = chain_name,
            chain = chain_name_2,
        ),
    };

    let res = register_composition(&config.server_url("/register/composition"), &chain_request)
        .expect("Failed to register composition");
    assert_eq!(res.status(), 200, "Failed to register composition '{}'", chain_name);

    let res2 = register_composition(&config.server_url("/register/composition"), &chain_request2)
        .expect("Failed to register composition");
    assert_eq!(res2.status(), 200, "Failed to register composition '{}'", chain_name_2);

    let matrix: Vec<u8> = vec![
        2i64, // two rows
        2, 0,
        0, 2,
    ]
    .iter()
    .flat_map(|x| x.to_le_bytes())
    .collect();

    // matmul_chain_composition calls matmul_chain twice, and matmul_chain
    // itself squares twice, so the input is squared 4 times: 2 -> 4 -> 16 ->
    // 256 -> 65536.
    let matrix_res: Vec<u8> = vec![
        2i64,
        65536, 0,
        0, 65536
    ].
    iter()
    .flat_map(|x| x.to_le_bytes())
    .collect();

    let mat_request = vec![InputSet {
        identifier: String::from(""),
        items: vec![InputItem {
            identifier: String::from(""),
            key: 0,
            data: matrix.clone(),
        }],
    }];

    // matmul_chain_composition applies matmul_chain twice (4 squarings total);
    // on the identity matrix the result must equal the input.
    let res = invoke_service(
        chain_name_2, config.lauberhorn_prog_num,
        config.lauberhorn_prog_ver, config.lauberhorn_proc_num, &config.lauberhorn_ip,
        config.lauberhorn_port, mat_request
    )
    .expect("Failed to invoke service");

    let res_exp = DandelionDeserializeResponse {
        sets: vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: matrix_res,
            }],
        }],
    };

    println!("{:?}", res);
    assert_eq!(bson::to_bson(&res_exp).unwrap(), bson::to_bson(&res).unwrap());

    let dereg_url = config.server_url("/deregister");
    assert_eq!(deregister(&dereg_url, chain_name_2).unwrap().status(), 200, "Failed to deregister composition");
    assert_eq!(deregister(&dereg_url, chain_name).unwrap().status(), 200, "Failed to deregister composition");
    assert_eq!(deregister(&dereg_url, function_name).unwrap().status(), 200, "Failed to deregister function");
}

fn get_engine_name() -> String {
    let name = if cfg!(feature = "mmu") {
    "Process".to_string()
    } else if cfg!(feature = "kvm") {
        "Kvm".to_string()
    } else {
        "default".to_string()
    };
    return name; 
}


#[test]
fn test_matmul() {
    let config = TestConfig::from_env();

    let req = get_matmul_reg_req(&config);

    // register binary
    let res = register_function(&config.server_url("/register/function"), &req)
        .expect("Failed to register function");

    assert_eq!(res.status(), 200);

    // dimension (squared matrix), then entries
    let mut data = vec![1i64, 1]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();

    let mat_request = vec![InputSet {
            identifier: String::from(""),
            items: vec![InputItem {
                identifier: String::from(""),
                key: 0,
                data: data,
            }],
        }];

    let res =
    invoke_service("matmul", 1, 1, 1, &config.lauberhorn_ip, config.lauberhorn_port, mat_request)
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
    println!("{:?}", res);
    assert_eq!(bson::to_bson(&res_exp).unwrap(), bson::to_bson(&res).unwrap());

    let dereg_url = config.server_url("/deregister");
    assert_eq!(deregister(&dereg_url, "matmul").unwrap().status(), 200, "Failed to deregister function");
}
