use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;
use dandelion_lauberhorn::lauberhorn::types::DandelionArgs;
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
    data: &DandelionArgs,
) -> Result<Vec<u8>, ()> {
    
    let body_bytes = bson::to_vec(&data).expect("BSON serialization failed");

    // create XDR stream for body
    let mut buffer = vec![0u8; 1500];
    let mut xdr_stream = xdr::XdrStream::new(xdr::XdrOp::Encode, &mut buffer);
    xdr_stream.set_string(function_id);
    xdr_stream.set_opaque(&body_bytes);

    let mut rpc_msg = oncrpc::OncRpcCall::new(xdr_stream.get_data());
    rpc_msg.set_identifier(prog_num, prog_ver, proc_num);

    let sock = UdpSocket::bind("10.0.0.5:0").map_err(|_| ())?;
    //sock.set_read_timeout(Some(Duration::from_secs(2))).map_err(|_| ())?;

    // Spawn listener thread before sending so we don't miss a fast response.
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let listener_sock = sock.try_clone().map_err(|_| ())?;
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 4096];
        if let Ok((size, _src)) = listener_sock.recv_from(&mut buf) {
            buf.truncate(size);
            let _ = tx.send(buf);
        }
    });

    sock.send_to(&rpc_msg.to_network_bytes(), format!("{}:{}", ip_addr, listen_port))
        .map_err(|_| ())?;

    let response = rx.recv_timeout(Duration::from_secs(2)).map_err(|_| ())?;
    if let Some(oncrpc::OncRpcMsg::Reply(response_msg)) = oncrpc::OncRpcMsg::from_network_bytes(&response) {
        let payload = response_msg.get_payload();
        let mut payload_vec = payload.to_vec();

        // todo. check lifetimes here...

        let mut xdr_stream = xdr::XdrStream::new(
            xdr::XdrOp::Decode,
            &mut payload_vec,
        );
        return xdr_stream.get_opaque().ok_or(());
    } else {
        debug!("Received invalid RPC response");
        return Err(());
    }
}
