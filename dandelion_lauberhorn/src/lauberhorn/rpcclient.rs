use byteorder::BigEndian;
use byteorder::{WriteBytesExt};


/// TODO(@Sven): if still needed, move to utils/oncrpc.rs
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
