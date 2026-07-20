//! Usage:
//!   latency [--rpc | --http] [ITERS] [WARMUP] [FUNCTION_NAME]

use std::fs::File;
use std::io::{BufWriter, Write};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use bson::to_vec;
use dandelion_lauberhorn::lauberhorn::marshal::{
    DandelionRPCRequest, DandelionRPCResponse, InputSets, RpcDecode, RpcEncode,
};
use dandelion_lauberhorn::utils::oncrpc;
use dandelion_lauberhorn::webserver::schemas::{InputItem, InputSet, RegisterChain, RegisterFunction};
use dandelion_server::DandelionRequest;
use reqwest::blocking::{Client, Response};

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

// to get around lifetime issues
#[derive(serde::Serialize, serde::Deserialize)]
struct DandelionServerRequest {
        name: String, 
        sets: Vec<InputSet>
}

enum Request {
    Rpc(DandelionRPCRequest),
    Http(DandelionServerRequest)
}
/*
matmul composition
*/
// try to reuse and import stuff from test/commons 
fn build_matmul_matmac_composition(http: bool) -> Request {

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

    if !http {
        Request::Rpc(DandelionRPCRequest {
            function_name: "graph".into(),
            data: InputSets {
                sets: input_sets
            }
        })
    } else {
        Request::Http(DandelionServerRequest {
            name: "graph".into(),
            sets: input_sets
        })
    }
}


fn verify_matmul_matmac_composition(resp: &DandelionRPCResponse) -> bool {
     let d = resp.sets[0].items[0].data
    .chunks(8)
    .map(|chunk| i64::from_le_bytes(chunk.try_into().unwrap()))
    .collect::<Vec<i64>>();

    println!("Output data: {:?}", d);
    assert_eq!(d, vec![2, -36, 52, -30, 66]);
    
    true
}

fn build_matmul(http: bool) -> Request {
    let data: Vec<u8> = vec![1i64, 1].iter().flat_map(|x| x.to_le_bytes()).collect();

    if !http {
        Request::Rpc(DandelionRPCRequest {
            function_name: "matmul".into(),
            data: InputSets {
                sets: vec![InputSet {
                    identifier: String::from(""),
                    items: vec![InputItem { identifier: String::from(""), key: 0, data }],
                }],
            },
        })
    } else {
        Request::Http(DandelionServerRequest {
            name: "matmul".into(),
            sets: vec![InputSet {
                    identifier: String::from(""),
                    items: vec![InputItem { identifier: String::from(""), key: 0, data }],
                }]
        })
    }
}

fn build_matmac(http: bool) -> Request {

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
    
    // for some reason, changed this to function_name .. ?? 
    // how does this even work ? cant put vec it ino
    if !http {
        Request::Rpc(DandelionRPCRequest {
            data: InputSets {
                sets: request   
            },
            function_name: "matmac".into()
        })
    } else {
        Request::Http(DandelionServerRequest { 
            name: "matmac".into(),
            sets: request
        })
    }

}

fn build_wire(config: &Config, function_name: &str) -> Vec<u8> {

    // argument representations we chan chose from 
    // matmul, matmac, matmul_matmac_composition
    let request = match function_name {
        "matmul" => build_matmul(config.use_http),
        "matmac" => build_matmac(config.use_http),
        "graph" => build_matmul_matmac_composition(config.use_http),
        // throw error here !
        _ => return vec![],
    };

    match request {
        Request::Http(r) => bson::to_vec(&r).unwrap(),
        Request::Rpc(r) => {
            let mut buffer = vec![0u8; 1500];
            let bytes_written = r
                .rpc_encode(buffer.as_mut_slice())
                .expect("rpc_encode failed");

            let mut rpc_msg = oncrpc::OncRpcCall::new(&buffer[..bytes_written as usize]);
            // put in config 
            rpc_msg.set_identifier(1, 1, 1);
            rpc_msg.to_network_bytes()
        }
    }
}


/// Send one request and block until the reply is decoded. Returns the measured
/// round-trip latency.
fn one_request(sock: &UdpSocket, wire: &[u8], dest: &str) -> Duration {
    let mut response = vec![0u8; 4096];

    let start = Instant::now();
    sock.send_to(wire, dest).expect("send_to failed");
    let received = sock.recv_from(&mut response).expect("recv_from failed");
    let elapsed = start.elapsed();

    // currently don't test result correctness apart from "is it deserializable"
    // result correctness tested in server_tests (for single invocation)
    response.truncate(received.0);
    let decoded_resp = match oncrpc::OncRpcMsg::from_network_bytes(&response) {
        Some(oncrpc::OncRpcMsg::Reply(msg)) => {
            DandelionRPCResponse::rpc_decode(msg.get_payload()).expect("decode failed")
        }
        _ => panic!("unexpected RPC response"),
    };
    //verify_matmul_matmac_composition(&decoded_resp);
    elapsed
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let rank = (p / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

pub fn register_function(url: &str, obj: &RegisterFunction) -> Result<Response, ()> {
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

pub fn register_composition(url: &str, obj: &RegisterChain) -> Result<Response, ()> {
    let client = Client::new();
    let body = bson::to_vec(obj).expect("BSON serialization failed");
    let res = client
        .post(url)
        .header("content-type", "application/octet-stream")
        .body(body)
        .send()
        .expect("Failed to send request");
    Ok(res)
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

fn get_reg_compositions(config: &Config) -> Vec<RegisterChain> {
        // just something random to test compositions
    let comp_str = r#"
    function matmul (mat_in) => (matmul_out);
    function matmac (mat_A, mat_B, mat_C) => (matmac_out);
    composition graph (matA, matB, matC, matD) => (matResult) {
        matmul(mat_in = all matA) => (mat1_out = matmul_out);
        matmul(mat_in = all matA) => (mat2_out = matmul_out);
        matmul(mat_in = all matA) => (mat3_out = matmul_out);
        matmul(mat_in = all matA) => (mat4_out = matmul_out);
        matmul(mat_in = all matA) => (mat5_out = matmul_out);
        matmul(mat_in = all matA) => (mat6_out = matmul_out);
        matmul(mat_in = all matA) => (mat7_out = matmul_out);
        matmul(mat_in = all matA) => (mat8_out = matmul_out);
        matmul(mat_in = all matA) => (mat9_out = matmul_out);

    }
"#;
    let comp_reg = RegisterChain {
        composition: comp_str.to_string()
    };

    vec![comp_reg]
}

fn get_reg_functions(config: &Config) -> Vec<RegisterFunction> {

    // MATMAC
    let path_matmac = config.test_binary("matmac");
    let matmac = RegisterFunction {
        name: "matmac".to_string(),
        context_size: 0x992_0000,
        engine_type: get_engine_name(),
        local_path: "".to_string(),
        binary: std::fs::read(path_matmac)
            .expect("Failed to read test function binary"),
        input_sets: vec![
            ("matrix_A".to_string(), None),
            ("matrix_B".to_string(), None),
            ("matrix_C".to_string(), None),
        ],
        output_sets: vec!["matrix_out".to_string()],
    };

    // MATMUL
    let path_matmul = config.test_binary("matmul");
    let matmul = RegisterFunction {
        name: "matmul".to_string(),
        context_size: 0x802_0000,
        engine_type: get_engine_name(),
        local_path: "".to_string(),
        binary: std::fs::read(path_matmul)
            .expect("Failed to read test function binary"),
        input_sets: vec![(String::from(""), None)],
        output_sets: vec![String::from("")],
    };

    vec![matmac, matmul]
}


fn one_http_request(client: &Client, endpoint: &str, body: &[u8]) -> Duration {
    let start = Instant::now();
    let resp = client
        .post(endpoint)
        .body(body.to_vec())
        .send()
        .expect("http send failed");
    assert!(resp.status().is_success(), "http request failed: {}", resp.status());
    let _ = resp.bytes().expect("failed to read response body");
    start.elapsed()
}

struct Config {
    use_http: bool, 
    n_iters: usize, 
    n_warmup: usize, 
    function_name: String, 
    lauberhorn_ip: String, 
    lauberhorn_port: u16,
    isolation: String, 
    dandelion_ip: String, 
    dandelion_port: u16, 
    binary_path: String,
    binary_arch: String,
}

impl Config {
    pub fn new() -> Self {
        Config {
            use_http: false, 
            n_iters: 1000,
            n_warmup: 100, 
            function_name: "matmul".into(),
            lauberhorn_ip: "10.0.0.5".into(),
            lauberhorn_port: 12345,
            isolation: "mmu".into(),
            dandelion_ip: "127.0.0.1".into(),
            dandelion_port: 6000,
            binary_path: "/mnt/extra_part/Code/dandelion/machine_interface/tests/data".into(),
            binary_arch: "x86_64".into()
        }
    }

    fn test_binary(&self, base_name: &str) -> String {
        format!("{}/test_elf_{}_{}_{}", self.binary_path, self.isolation, self.binary_arch, base_name)
    }
}

fn main() {
    // Args: [--rpc | --http] [ITERS] [WARMUP] [FUNCTION_NAME]

    let args: Vec<String> = std::env::args().skip(1).collect();
    
    let use_http = args.iter().any(|a| a == "--http");
    if use_http && args.iter().any(|a| a == "--rpc") {
        panic!("pass only one of --rpc / --http");
    }

    let mut config = Config::new();
    let mut args_iter = args.iter().peekable();

    while let Some(arg) = args_iter.next() {
        match arg.as_str() {
            "--rpc" => config.use_http = false,
            "--http" => config.use_http = true,
            "--nwarmup" => {
                if let Some(next_val) = args_iter.next() {
                    config.n_warmup = next_val.parse::<usize>().expect("Invalid number for --nwarmup");
                }
            }
            "--niters" => {
                if let Some(next_val) = args_iter.next() {
                    config.n_iters = next_val.parse::<usize>().expect("Invalid number for --niters");
                }
            }
            "--function" => {
                if let Some(next_val) = args_iter.next() {
                    config.function_name = next_val.to_string();
                }
            }
            "--lauberhorn-ip" => {
                if let Some(next_val) = args_iter.next() {
                    config.function_name = next_val.to_string();
                }
            }
            "--lauberhorn-port" => {
                if let Some(next_val) = args_iter.next() {
                    config.lauberhorn_port = next_val.parse::<u16>().expect("invalid number for --lauberhorn-port");
                }
            }   
            "--dandelion-port" => {
                if let Some(next_val) = args_iter.next() {
                    config.dandelion_port = next_val.parse::<u16>().expect("invalid number for --dandelion-port");
                }
            } 
            "--dandelion-ip" => {
                if let Some(next_val) = args_iter.next() {
                    config.dandelion_ip = next_val.to_string();
                }
            }
            "--isolation" => {
                if let Some(next_val) = args_iter.next() {
                    config.isolation = next_val.to_string();
                }
            }
            "--binary-path" => {
                if let Some(next_val) = args_iter.next() {
                    config.binary_path = next_val.to_string();
                }
            }
            "--binary-arch" => {
                if let Some(next_val) = args_iter.next() {
                    config.binary_arch = next_val.to_string();
                }
            }                             
            _ => {} // Ignore unknown arguments or the binary name
        }
    }

    // register functions and compositions
    for f in &get_reg_functions(&config) {
        let _ = register_function(format!("http://{}:{}/register/function", &config.dandelion_ip, config.dandelion_port).as_str(), f);
    }

    for c in &get_reg_compositions(&config) {
        let _ = register_composition(format!("http://{}:{}/register/composition", &config.dandelion_ip, config.dandelion_port).as_str(), c);
    }

    let mode = if use_http { "http" } else { "rpc" };
    let mut samples = Vec::with_capacity(config.n_iters);

    if use_http {
        let endpoint: String = format!("http://{}:{}/execute", &config.dandelion_ip, config.dandelion_port);
        let client = Client::new();
        let body = build_wire(&config, &config.function_name);
        println!("[http] target {}:{}, function \"{}\"", &config.dandelion_ip, config.dandelion_port, &config.function_name);

        println!("Warming up ({} requests)...", config.n_warmup);
        for _ in 0..config.n_warmup {
            one_http_request(&client, &endpoint, &body);
        }
        println!("Measuring {} requests...", config.n_iters);
        for _ in 0..config.n_iters {
            samples.push(one_http_request(&client, &endpoint, &body));
        }
    } else {

        let dest = format!("{}:{}", &config.lauberhorn_ip, &config.lauberhorn_port);

        // get payload
        let wire = build_wire(&config, &config.function_name);

        // bind to random port for sending
        let sock = UdpSocket::bind(format!("127.0.0.1:9999")).expect("bind failed");
        sock.set_read_timeout(Some(Duration::from_secs(5))).expect("set_read_timeout failed");
        println!("[{mode}] target {dest}, function \"{}\"", &config.function_name);

        println!("Warming up ({} requests)...", config.n_warmup);
        for _ in 0..config.n_warmup {
            one_request(&sock, &wire, &dest);
        }
        println!("Measuring {} requests...", config.n_iters);
        for _ in 0..config.n_iters {
            samples.push(one_request(&sock, &wire, &dest));
        }
    }

    // dump data to file
    let file = File::create("latency_samples.txt").expect("Unable to create file");
    let mut writer = BufWriter::new(file);

    for sample in &samples {
        let us = sample.as_secs_f64() * 1e6;
        writeln!(writer, "{:.2}", us).expect("Unable to write data");
    }
    
    samples.sort();
    let sum: Duration = samples.iter().sum();
    let mean = sum / samples.len() as u32;

    let us = |d: Duration| d.as_secs_f64() * 1e6;
    println!("\n=== single-request latency (n = {}) ===", samples.len());
    println!("  min   : {:9.2} us", us(samples[0]));
    println!("  mean  : {:9.2} us", us(mean));
    println!("  p50   : {:9.2} us", us(percentile(&samples, 50.0)));
    println!("  p90   : {:9.2} us", us(percentile(&samples, 90.0)));
    println!("  p99   : {:9.2} us", us(percentile(&samples, 99.0)));
    println!("  max   : {:9.2} us", us(samples[samples.len() - 1]));
}
