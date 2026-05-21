use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::c_void;
use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;
use dandelion_lauberhorn::lauberhorn::marshal::dandelion_marshal_call;
use dandelion_lauberhorn::lauberhorn::marshal::dandelion_unmarshal_resp;
use dandelion_lauberhorn::lauberhorn::types::DandelionArgs;
use dandelion_lauberhorn::lauberhorn::types::DandelionRPCRequest;
use dandelion_lauberhorn::utils::xdr::xdr_write_opaque;
use dandelion_lauberhorn::webserver::schemas::DandelionDeserializeResponse;
use log::{debug};
use reqwest::blocking::{Client, Response};
use dandelion_lauberhorn::webserver::schemas::{RegisterFunction, RegisterService};
use bson::ser::to_vec;
use dandelion_lauberhorn::utils::xdr;
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
    data: DandelionArgs,
) -> Result<DandelionDeserializeResponse, String> {
    // create XDR stream for body
    let mut buffer = vec![0u8; 1500];

    let request = DandelionRPCRequest {
        function_name: function_id.into(),
        payload: data
    };

    // Marshal the request and get number of bytes written
    let bytes_written = unsafe {
        dandelion_marshal_call(
            &request as *const DandelionRPCRequest as *const c_void, 
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len(), 
            0 as *const c_void
        )
    };
    
    if bytes_written == 0 {
        return Err("marshal_call failed to write data".to_string());
    }
    
    
    // Use ONLY the marshalled bytes, not the entire buffer1
    let mut rpc_msg = oncrpc::OncRpcCall::new(&buffer[..bytes_written as usize]);
    rpc_msg.set_identifier(prog_num, prog_ver, proc_num);

    let sock = UdpSocket::bind("10.0.0.5:0").map_err(|e| {
        let msg = format!("Failed to bind UDP socket: {}", e);
        eprintln!("{}", msg);
        msg
    })?;
    
    let local_addr = sock.local_addr().map_err(|e| {
        msg
    })?;

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let listener_sock = sock.try_clone().map_err(|e| {
        eprintln!("{}", msg);
        msg
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
            msg
        })?;
    

    let response = rx.recv_timeout(Duration::from_secs(2)).map_err(|e| {
        msg
    })?;

    if let Some(oncrpc::OncRpcMsg::Reply(response_msg)) = oncrpc::OncRpcMsg::from_network_bytes(&response) {
        let payload = response_msg.get_payload();
        let mut reply_buffer: *mut c_void = std::ptr::null_mut();
        unsafe {
            dandelion_unmarshal_resp(
                payload.as_ptr() as *const c_void, &mut reply_buffer,
                payload.len(), 0 as *const c_void
            );
        }
        if reply_buffer.is_null() {
            let msg = "FFI dandelion_unmarshal_resp returned null pointer".to_string();
            eprintln!("{}", msg);
            return Err(msg);
        }
        let reply = unsafe { Box::from_raw(reply_buffer as *mut DandelionDeserializeResponse) };
        Ok(*reply)
    } else {
        Err(msg)
    }
}
