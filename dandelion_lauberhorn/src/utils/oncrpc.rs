use std::borrow::Cow;
use rand::{Rng};

pub enum OncRpcMsg<'a> {
    Call(OncRpcCall<'a>),
    Reply(OncRpcReply<'a>),
}

pub struct OncRpcCall<'a> {
    header: OncRpcHeader,
    payload: Cow<'a, [u8]>,
}

pub struct OncRpcReply<'a> {
    header: OncRpcReplyHeader,
    payload: Cow<'a, [u8]>,
}

/// Onc RPC Header for RPC Version 2
struct OncRpcHeader {
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

struct OncRpcReplyHeader {
    pub xid: u32,
    pub msg_type: u32,
    pub msg_stat: u32,
    pub verf_flavor: u32,
    pub verf_length: u32,
    pub accept_stat: u32,
}


impl OncRpcReplyHeader {
    pub fn to_network_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; REPLY_HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.xid.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.msg_type.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.msg_stat.to_be_bytes());
        bytes[12..16].copy_from_slice(&self.verf_flavor.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.verf_length.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.accept_stat.to_be_bytes());
        bytes
    }

    pub fn from_network_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < REPLY_HEADER_SIZE {
            return None
        }
        Some(Self {
            xid: u32::from_be_bytes(data[0..4].try_into().unwrap()),
            msg_type: u32::from_be_bytes(data[4..8].try_into().unwrap()),
            msg_stat: u32::from_be_bytes(data[8..12].try_into().unwrap()),
            verf_flavor: u32::from_be_bytes(data[12..16].try_into().unwrap()),
            verf_length: u32::from_be_bytes(data[16..20].try_into().unwrap()),
            accept_stat: u32::from_be_bytes(data[20..24].try_into().unwrap()),
        })
    }
}

impl OncRpcHeader {
    pub fn to_network_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.xid.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.msg_type.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.rpc_version.to_be_bytes());  
        bytes[12..16].copy_from_slice(&self.prog_num.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.prog_ver.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.proc_num.to_be_bytes());
        bytes[24..28].copy_from_slice(&self.cred_flavor.to_be_bytes());
        bytes[28..32].copy_from_slice(&self.cred_length.to_be_bytes());
        bytes[32..36].copy_from_slice(&self.verf_flavor.to_be_bytes());
        bytes[36..40].copy_from_slice(&self.verf_length.to_be_bytes());
        bytes
    }

    pub fn from_network_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < HEADER_SIZE {
            return None
        }
        Some(Self {
            xid: u32::from_be_bytes(data[0..4].try_into().unwrap()),
            msg_type: u32::from_be_bytes(data[4..8].try_into().unwrap()),
            rpc_version: u32::from_be_bytes(data[8..12].try_into().unwrap()),
            prog_num: u32::from_be_bytes(data[12..16].try_into().unwrap()),
            prog_ver: u32::from_be_bytes(data[16..20].try_into().unwrap()),
            proc_num: u32::from_be_bytes(data[20..24].try_into().unwrap()),
            cred_flavor: u32::from_be_bytes(data[24..28].try_into().unwrap()),
            cred_length: u32::from_be_bytes(data[28..32].try_into().unwrap()),
            verf_flavor: u32::from_be_bytes(data[32..36].try_into().unwrap()),
            verf_length: u32::from_be_bytes(data[36..40].try_into().unwrap()),
        })
    }
}

#[derive(Clone, Copy)]
pub enum MsgType {
    Call = 0,
    Reply = 1,
}

static HEADER_SIZE: usize = 40;
static REPLY_HEADER_SIZE: usize = 24;

fn create_xid() -> u32 {
    let mut rng  = rand::rng();
    rng.next_u32()
}

impl<'a> OncRpcCall<'a> {
    pub fn new(payload: &[u8]) -> Self {
        let mut msg = Self {
            header: OncRpcHeader {
                xid: 0,
                msg_type: 0,
                rpc_version: 0,
                prog_num: 0,
                prog_ver: 0,
                proc_num: 0,
                cred_flavor: 0,
                cred_length: 0,
                verf_flavor: 0,
                verf_length: 0,
            },
            payload: Cow::Owned(payload.to_vec()),
        };

        msg.set_msg_type(MsgType::Call);
        msg.set_xid(create_xid());
        msg.set_version(2);
        msg
    }

    pub fn from_network_bytes(data: &'a [u8]) -> Option<Self> {
        
        let msg = Self {
            header: OncRpcHeader::from_network_bytes(data)?,
            payload: Cow::Borrowed(data[HEADER_SIZE..].as_ref()),
        };

        if msg.header.msg_type != MsgType::Call as u32 {
            return None;
        }
        if msg.header.cred_length != 0 || msg.header.verf_length != 0 {
            return None
        }

        Some(msg)
    }

    pub fn to_network_bytes(&self) -> Vec<u8> {
        let mut out = self.header.to_network_bytes();
        out.extend_from_slice(self.payload.as_ref());
        out
    }

    pub fn get_xid(&self) -> u32 {
        self.header.xid
    }

    pub fn get_payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    pub fn set_xid(&mut self, xid: u32) {
        self.header.xid = xid;
    }

    fn set_version(&mut self, version: u32) {
        self.header.rpc_version = version;
    }

    pub fn set_identifier(&mut self, prog_num: u32, prog_ver: u32, proc_num: u32) {
        self.header.prog_num = prog_num;
        self.header.prog_ver = prog_ver;
        self.header.proc_num = proc_num;
    }

    pub fn set_msg_type(&mut self, msg_type: MsgType) {
        self.header.msg_type = msg_type as u32;
    }
}

impl<'a> OncRpcReply<'a> {
    pub fn from_network_bytes(data: &'a [u8]) -> Option<Self> {
        let header = OncRpcReplyHeader::from_network_bytes(data)?;

        if header.msg_type != MsgType::Reply as u32 {
            return None;
        }
        // For now we only support accepted+success replies.
        if header.msg_stat != 0 || header.accept_stat != 0 {
            return None;
        }
        // Current helper assumes no verifier body.
        if header.verf_length != 0 {
            return None;
        }

        Some(Self {
            header,
            payload: Cow::Borrowed(data[REPLY_HEADER_SIZE..].as_ref()),
        })
    }

    pub fn get_xid(&self) -> u32 {
        self.header.xid
    }

    pub fn get_payload(&self) -> &[u8] {
        self.payload.as_ref()
    }
}

/// Onc RPC Message wrapper
impl<'a> OncRpcMsg<'a> {

    pub fn from_network_bytes(data: &'a [u8]) -> Option<Self> {
        if data.len() < 8 {
            return None
        }

        match u32::from_be_bytes(data[4..8].try_into().unwrap()) {
            x if x == MsgType::Call as u32 => {
                OncRpcCall::from_network_bytes(data).map(OncRpcMsg::Call)
            }
            x if x == MsgType::Reply as u32 => {
                OncRpcReply::from_network_bytes(data).map(OncRpcMsg::Reply)
            }
            _ => None,
        }
    }

    pub fn get_payload(&self) -> &[u8] {
        match self {
            OncRpcMsg::Call(c) => c.get_payload(),
            OncRpcMsg::Reply(r) => r.get_payload(),
        }
    }
}