use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;
use dandelion_lauberhorn::lauberhorn::marshal::{DandelionRPCResponse, InputSets, RpcDecode, RpcEncode};
use dandelion_lauberhorn::lauberhorn::{marshal::DandelionRPCRequest};
use reqwest::blocking::{Client, Response};
use dandelion_lauberhorn::webserver::schemas::{InputSet, RegisterFunction, RegisterService};
use bson::ser::to_vec;
use dandelion_lauberhorn::utils::oncrpc;


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

pub fn register_service(url: &str, obj: &RegisterService) -> Result<Response, ()> {
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

pub fn invoke_service(
    function_id: &str,
    prog_num: u32,
    prog_ver: u32,
    proc_num: u32,
    ip_addr: &str,
    listen_port: u16,
    data: Vec<InputSet>,
) -> Result<DandelionRPCResponse, ()> {
    // create XDR stream for body
    let mut buffer = vec![0u8; 1500];

    let request = DandelionRPCRequest {
        function_name: function_id.into(),
        data: InputSets {
            sets: data
        }
    };

    // Marshal the request and get number of bytes written
    let bytes_written = request.rpc_encode(buffer.as_mut_slice())
        .map_err(|e| { eprintln!("rpc_encode failed: {:?}", e); })?;

    // Use ONLY the marshalled bytes, not the entire buffer
    let mut rpc_msg = oncrpc::OncRpcCall::new(&buffer[..bytes_written as usize]);
    rpc_msg.set_identifier(prog_num, prog_ver, proc_num);

    let sock = UdpSocket::bind("10.0.0.5:0").map_err(|e| {
        eprintln!("bind failed: {}", e); ()
    })?;
    
    let local_addr = sock.local_addr().map_err(|e| {
        ()
    })?;

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let listener_sock = sock.try_clone().map_err(|e| {
        ()
    })?;
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 4096];
        match listener_sock.recv_from(&mut buf) {
            Ok((size, src)) => {
                eprintln!("[listener] Received {} bytes from {}", size, src);
                buf.truncate(size);
                let _ = tx.send(buf);
            }
            Err(e) => {
            }
        }
    });

    sock.send_to(&rpc_msg.to_network_bytes(), format!("{}:{}", ip_addr, listen_port))
        .map_err(|e| {
            ()
        })?;
    

    let response = rx.recv_timeout(Duration::from_secs(2)).map_err(|e| {
        eprintln!("recv_timeout: {}", e); ()
    })?;

    if let Some(oncrpc::OncRpcMsg::Reply(response_msg)) = oncrpc::OncRpcMsg::from_network_bytes(&response) {
        let payload = response_msg.get_payload();

        let reply = DandelionRPCResponse::rpc_decode(payload)
            .unwrap();

        Ok(reply)
    } else {
        Err(())
    }
}
