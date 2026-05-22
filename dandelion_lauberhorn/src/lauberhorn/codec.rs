use std::{ffi::{CString, c_char}, marker::PhantomData};

use crate::lauberhorn::{
    ffi::{LauberhornRpcEndpointRaw, MarshalProc, RpcCodec, RpcOps, UnmarshalProc},
    marshal::{RpcDecode, RpcEncode,
         dandelion_free, dandelion_marshal, dandelion_unmarshal}
};

pub struct LauberhornRpcEndpoint<In: RpcDecode, Out: RpcEncode> {
    pub(crate) inner: LauberhornRpcEndpointRaw,
    pub(crate) _marker: PhantomData<(In, Out)>
}

unsafe impl<In: RpcDecode, Out: RpcEncode> Sync for LauberhornRpcEndpoint<In, Out>{}
unsafe impl<In: RpcDecode, Out: RpcEncode> Send for LauberhornRpcEndpoint<In, Out> {}

// require this to be passed for servicee reg to (addr should be optional, (bind to 0.0.0.0 !)
// can have server, client as helpers
// ENDPOINT 

impl RpcOps {

    pub const fn for_types<Out: RpcEncode, In: RpcDecode>() -> Self {
        RpcOps {
            marshal_call: dandelion_marshal::<Out> as MarshalProc, 
            unmarshal_call: dandelion_unmarshal::<In> as UnmarshalProc, 
            marshal_resp: dandelion_marshal::<Out> as MarshalProc,
            unmarshal_resp: dandelion_unmarshal::<In> as UnmarshalProc,
            free: dandelion_free
        }
    }
}


impl<In: RpcDecode, Out: RpcEncode> LauberhornRpcEndpoint<In, Out> {

    pub fn new(
        daddr: &str,
        dport: u16,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        // should also pass codec here explicitly
    ) -> Self {

        let daddr_ptr = CString::new(daddr).unwrap().into_raw();
        // TODO(@Sven): fix directions (out, in swapped)
        let ops = Box::into_raw(Box::new(RpcOps::for_types::<Out, In>())); // should be passed explicitly !
        

        let inner = LauberhornRpcEndpointRaw {
            daddr: daddr_ptr,
            dport,
            prog_num,
            prog_ver,
            proc_num,
            codec: RpcCodec {
                ops,
                private: std::ptr::null(),
            },
        };
        LauberhornRpcEndpoint { inner, _marker: std::marker::PhantomData, }
    }
}

impl<In: RpcDecode, Out: RpcEncode> Drop for LauberhornRpcEndpoint<In, Out> {
    fn drop(&mut self) {
        unsafe {
            if !self.inner.codec.ops.is_null() {
                drop(Box::from_raw(self.inner.codec.ops as *mut RpcOps));
            }
            if !self.inner.daddr.is_null() {
                drop(CString::from_raw(self.inner.daddr as *mut c_char));
            }
        }
    }
}


