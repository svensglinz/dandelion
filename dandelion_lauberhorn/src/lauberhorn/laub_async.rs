use std::{collections::HashMap, ffi::c_void};

use crate::lauberhorn::{codec::LauberhornRpcEndpoint, ffi::{AwaitSetRaw, LauberhornRpcEndpointRaw, RpcResult, RpcStatus, lauberhorn_await_any, lauberhorn_call_async, lauberhorn_free_result}, marshal::{RpcDecode, RpcEncode}};

#[repr(C)]
pub struct AwaitSet<T: RpcDecode> {
    inner: AwaitSetRaw,
    handles: HashMap<usize, AsyncCallHandle<T>>,
}

impl<T: RpcDecode> AwaitSet<T> {
    pub fn new() -> Self {
        AwaitSet {
            inner: AwaitSetRaw { rpc_mask: 0 },
            handles: HashMap::new(),
        }
    }

    pub fn add(&mut self, handle: AsyncCallHandle<T>) {
        self.inner.rpc_mask |= 1 << handle.id;
        self.handles.insert(handle.id, handle);
    }

    pub fn del(&mut self, id: usize) {
        self.inner.rpc_mask &= !(1 << id);
        self.handles.remove(&id);
    }

    pub fn size(&self) -> u32 {
        self.handles.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    pub fn await_any(&mut self) -> Option<AsyncCallHandle<T>> {
        if self.is_empty() { return None; }

        let mut result = RpcResult::empty();
        let ok = unsafe {
            lauberhorn_await_any(
                &mut self.inner as *mut AwaitSetRaw,
                &mut result as *mut RpcResult,
            )
        };
        if !ok { 
            return None; 
        }

        let mut handle = self.handles.remove(&(result.id as usize))?;

        match result.status {
            RpcStatus::RpcOk => {
                // claim ownership of memory back 
                // no need to invoke lauberhorn_free_result
                let value: Box<T> = unsafe { Box::from_raw(result.data as *mut T) };
                handle.data = Some(value);
                result.data = std::ptr::null_mut();
                Some(handle)
            }
            RpcStatus::RpcTimeout | RpcStatus::RpcError => {
                handle.status = result.status;
                Some(handle)
            }
        }
    }

}


pub struct AsyncCallHandle<T: RpcDecode> {
    pub id: usize,
    pub status: RpcStatus,
    data: Option<Box<T>>,
}

impl<T: RpcDecode> AsyncCallHandle<T> {
    pub fn new(id: usize) -> Self {
        AsyncCallHandle { id: id, status: RpcStatus::RpcOk, data: None }
    }
    
    pub fn take_data(&mut self) -> Option<Box<T>> {
        self.data.take()
    }

    pub fn is_complete(&self) -> bool {
        self.data.is_some()
    }
}

pub fn call_async<Req: RpcEncode, Resp: RpcDecode>(
    ep: &LauberhornRpcEndpoint<Req, Resp>,
    payload: &Req,
) -> Result<AsyncCallHandle<Resp>, ()> {
    let res = unsafe {
        // from ffi
        lauberhorn_call_async(
            &ep.inner as *const LauberhornRpcEndpointRaw,
            payload as *const Req as *const c_void,
            std::mem::size_of::<Req>(),
        )
    };

    if res < 0 {
        Err(())
    } else {
        Ok(AsyncCallHandle::new(res as usize))
    }
}

// call_cb not implemented as not needed