use std::{collections::HashMap, sync::{Arc, Mutex}};
use byteorder::BigEndian;
use std::net::UdpSocket;
use byteorder::{WriteBytesExt, ReadBytesExt};

pub struct OncRpcClient {
    socket: Arc<UdpSocket>,
    pending: Arc<Mutex<HashMap<u32, std::sync::mpsc::SyncSender<(u32, Vec<u8>)>>>>,
}

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

fn parse_rpc_response(response: &Vec<u8>) -> Result<(OncRpcHeader, Vec<u8>), ()> {
    let mut cursor = std::io::Cursor::new(response);

    if response.len() < std::mem::size_of::<OncRpcHeader>() {
        return Err(());
    }

    let xid = cursor.read_u32::<BigEndian>().unwrap();
    let msg_type = cursor.read_u32::<BigEndian>().unwrap();
    let rpc_version = cursor.read_u32::<BigEndian>().unwrap();
    let prog_num = cursor.read_u32::<BigEndian>().unwrap();
    let prog_ver = cursor.read_u32::<BigEndian>().unwrap();
    let proc_num = cursor.read_u32::<BigEndian>().unwrap();

    let cred_flavor = cursor.read_u32::<BigEndian>().unwrap();
    let cred_length = cursor.read_u32::<BigEndian>().unwrap();
    // currently only support no credentials --> cred_length must be 0
    if cred_length != 0 {
        return Err(());
    }

    let verf_flavor = cursor.read_u32::<BigEndian>().unwrap();
    let verf_length = cursor.read_u32::<BigEndian>().unwrap();
    // currently only support no verifier --> verf_length must be 0
    if verf_length != 0 {
        return Err(());
    }

    let header = OncRpcHeader {
        xid,
        msg_type,
        rpc_version,
        prog_num,
        prog_ver,
        proc_num,
        cred_flavor,
        cred_length,
        verf_flavor,
        verf_length,
    };

    let payload_start = std::mem::size_of::<OncRpcHeader>();
    let payload = response[payload_start..].to_vec();

    Ok((header, payload))
}

impl OncRpcClient {
    pub fn new(socket: UdpSocket) -> Self {
        let socket = Arc::new(socket);
        
        // replace mutex with refcell as we dont access this from multiple threads !
        let pending = Arc::new(
            Mutex::new(HashMap::<u32, std::sync::mpsc::SyncSender<(u32, Vec<u8>)>>::new()));
        let recv_socket = socket.clone();
        let pending_map = pending.clone();

        // poll for responses in a background thread
        // and match them to pending requests using the xid
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                // blocking recv
                let (len, _addr) = recv_socket.recv_from(&mut buf).unwrap();
                let response = buf[..len].to_vec();
                let (header, payload) = parse_rpc_response(&response).unwrap();
                let xid = header.xid; 

                if let Some(tx) = pending_map.lock().unwrap().remove(&xid) {
                    let _ = tx.send((xid, payload));
                } else {
                    eprintln!("Received response for unknown xid: {}", xid);
                }
            }
        });
        Self { socket, pending }
    }

    pub fn send_request_async(
        &self, xid: u32,
        data: &Vec<u8>,
        dest: &str, done_tx: std::sync::mpsc::SyncSender<(u32, Vec<u8>)>) -> std::io::Result<usize> {
        self.pending.lock().unwrap().insert(xid, done_tx);
        self.socket.send_to(data, dest)
    }

    pub fn send_request_sync(&self, xid: u32, data: &[u8], dest: &str) -> Vec<u8> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.pending.lock().unwrap().insert(xid, tx);
        self.socket.send_to(data, dest).unwrap();
        let (_, response) = rx.recv().unwrap();
        response
    }
}