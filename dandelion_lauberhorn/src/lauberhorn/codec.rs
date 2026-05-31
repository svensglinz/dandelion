use std::{ffi::{CString, c_char}, marker::PhantomData};

use crate::lauberhorn::{
    ffi::{LauberhornRpcEndpointRaw, MarshalProc, RpcCodec, RpcOps, UnmarshalProc},
    marshal::{RpcDecode, RpcEncode,
         dandelion_free, dandelion_marshal, dandelion_unmarshal}
};

pub struct LauberhornRpcEndpoint<Req: RpcEncode, Resp: RpcDecode> {
    pub(crate) inner: LauberhornRpcEndpointRaw,
    pub(crate) _marker: PhantomData<(Req, Resp)>
}

unsafe impl<Req: RpcEncode, Resp: RpcDecode> Sync for LauberhornRpcEndpoint<Req, Resp>{}
unsafe impl<Req: RpcEncode, Resp: RpcDecode> Send for LauberhornRpcEndpoint<Req, Resp> {}

// require this to be passed for servicee reg to (addr should be optional, (bind to 0.0.0.0 !)
// can have server, client as helpers
// ENDPOINT 

impl RpcOps {

    pub const fn for_server<Req: RpcDecode, Resp: RpcEncode>() -> Self {
        RpcOps {
            marshal_call:   dandelion_marshal::<Resp>   as MarshalProc,   // unused on server
            unmarshal_call: dandelion_unmarshal::<Req>  as UnmarshalProc, // decode incoming request
            marshal_resp:   dandelion_marshal::<Resp>   as MarshalProc,   // encode response
            unmarshal_resp: dandelion_unmarshal::<Req>  as UnmarshalProc, // unused on server
            free: dandelion_free,
        }
    }

    pub const fn for_client<Req: RpcEncode, Resp: RpcDecode>() -> Self {
        RpcOps {
            marshal_call:   dandelion_marshal::<Req>    as MarshalProc,   // encode request
            unmarshal_call: dandelion_unmarshal::<Resp> as UnmarshalProc, // unused on client
            marshal_resp:   dandelion_marshal::<Req>    as MarshalProc,   // unused on client
            unmarshal_resp: dandelion_unmarshal::<Resp> as UnmarshalProc, // decode response
            free: dandelion_free,
        }
    }
}


impl<Req: RpcEncode, Resp: RpcDecode> LauberhornRpcEndpoint<Req, Resp> {

    pub fn new(
        daddr: &str,
        dport: u16,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        ops: *mut RpcOps,
        // should also pass codec here explicitly
    ) -> Self {

        let daddr_ptr = CString::new(daddr).unwrap().into_raw();
        // TODO(@Sven): fix directions (out, in swapped)       

        // TODO(@Sven): memory leak ?
        let codec = Box::into_raw(Box::new(RpcCodec {
            ops, 
            private: std::ptr::null()
        }));

        let inner = LauberhornRpcEndpointRaw {
            daddr: daddr_ptr,
            dport,
            prog_num,
            prog_ver,
            proc_num,
            codec: codec
        };
        LauberhornRpcEndpoint { inner, _marker: std::marker::PhantomData, }
    }
}

// impl<Req: RpcEncode, Resp: RpcDecode> Drop for LauberhornRpcEndpoint<Req, Resp> {
//     fn drop(&mut self) {
//         unsafe {
//             if !self.inner.codec.ops.is_null() {
//                 drop(Box::from_raw(self.inner.codec.ops as *mut RpcOps));
//             }
//             if !self.inner.daddr.is_null() {
//                 drop(CString::from_raw(self.inner.daddr as *mut c_char));
//             }
//         }
//     }
// }


