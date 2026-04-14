use std::net::UdpSocket;
use byteorder::{BigEndian, WriteBytesExt};
use dandelion_server::DandelionRequest;
use reqwest::blocking::{Client, Response};
use dandelion_lauberhorn::webserver::schemas::{RegisterFunction, RegisterService};
use bson::ser::to_vec;

pub struct OncRpcHeader {
    pub xid: u32,
    pub msg_type: u32,
    pub rpc_version: u32,
    pub prog_num: u32,
    pub prog_ver: u32,
    pub proc_num: u32,
    pub cred_flavor: u32,
    pub cred_length: u32,
    pub verf_flavor: u32,
    pub verf_length: u32,
}

impl OncRpcHeader {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.write_u32::<BigEndian>(self.xid).unwrap();
        bytes.write_u32::<BigEndian>(self.msg_type).unwrap();
        bytes.write_u32::<BigEndian>(self.rpc_version).unwrap();
        bytes.write_u32::<BigEndian>(self.prog_num).unwrap();
        bytes.write_u32::<BigEndian>(self.prog_ver).unwrap();
        bytes.write_u32::<BigEndian>(self.proc_num).unwrap();
        bytes.write_u32::<BigEndian>(self.cred_flavor).unwrap();
        bytes.write_u32::<BigEndian>(self.cred_length).unwrap();
        bytes.write_u32::<BigEndian>(self.verf_flavor).unwrap();
        bytes.write_u32::<BigEndian>(self.verf_length).unwrap();
        bytes
    }
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
    data: &DandelionRequest,
) -> Result<(), ()> {
    let header = OncRpcHeader {
        xid: 0,
        msg_type: 0,
        rpc_version: 2,
        prog_num,
        prog_ver,
        proc_num,
        cred_flavor: 0,
        cred_length: 0,
        verf_flavor: 0,
        verf_length: 0,
    };

    let header_bytes = header.to_bytes();

    let body_bytes = to_vec(&data).expect("BSON serialization failed");
    let mut request_bytes = header_bytes;
    request_bytes.extend(body_bytes);

    let sock = UdpSocket::bind("10.0.0.5:0")
        .expect("Failed to bind UDP socket");
    sock.send_to(&request_bytes, format!("{}:{}", ip_addr, listen_port))
        .expect("Failed to send UDP request");

    Ok(())
}
