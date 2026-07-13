use std::{ffi::{CString}, marker::PhantomData};

use crate::lauberhorn::{
    ffi::{AllocProc, FreeProc, LauberhornRpcEndpointRaw, MarshalProc, RpcCodec, RpcOps, UnmarshalProc},
    marshal::{RpcDecode, RpcEncode, dandelion_alloc, dandelion_free, dandelion_marshal, dandelion_unmarshal}
};

pub struct LauberhornRpcEndpoint<Req: RpcEncode, Resp: RpcDecode> {
    pub(crate) inner: LauberhornRpcEndpointRaw,
    pub(crate) _marker: PhantomData<(Req, Resp)>
}

unsafe impl<Req: RpcEncode, Resp: RpcDecode> Sync for LauberhornRpcEndpoint<Req, Resp>{}
unsafe impl<Req: RpcEncode, Resp: RpcDecode> Send for LauberhornRpcEndpoint<Req, Resp> {}


impl RpcOps {

    // unmarshal Req, marhsal Resp
    pub const fn for_server<Req: RpcDecode, Resp: RpcEncode>() -> Self {
        RpcOps {
            marshal_call:   dandelion_marshal::<Resp>   as MarshalProc, 
            marshal_resp:   dandelion_marshal::<Resp>   as MarshalProc, 
            unmarshal_call: dandelion_unmarshal::<Req>  as UnmarshalProc, 
            unmarshal_resp: dandelion_unmarshal::<Req>  as UnmarshalProc, 
            alloc:          dandelion_alloc::<Req>      as AllocProc,
            free:           dandelion_free::<Req>       as FreeProc,
        }
    }

    // marshal Req and unmarshal Resp
    pub const fn for_client<Req: RpcEncode, Resp: RpcDecode>() -> Self {
        RpcOps {
            marshal_call:   dandelion_marshal::<Req>    as MarshalProc,  
            marshal_resp:   dandelion_marshal::<Req>    as MarshalProc,  
            unmarshal_call: dandelion_unmarshal::<Resp> as UnmarshalProc, 
            unmarshal_resp: dandelion_unmarshal::<Resp> as UnmarshalProc, 
            alloc:          dandelion_alloc::<Resp>     as AllocProc,  
            free:           dandelion_free::<Resp>      as FreeProc, 
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
