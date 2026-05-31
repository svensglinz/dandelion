use std::{collections::HashMap, ffi::c_void};

use crate::lauberhorn::{codec::LauberhornRpcEndpoint,
    ffi::{AwaitSetRaw, LauberhornCompletion, LauberhornRpcEndpointRaw,
    lauberhorn_await_any, lauberhorn_call_async}, 
    marshal::{RpcDecode, RpcEncode}};

#[repr(C)]
pub struct AwaitSet<T: RpcDecode> {
    inner: AwaitSetRaw,
    handles: HashMap<usize, AsyncCallHandle<T>>,
}

impl<T: RpcDecode> AwaitSet<T> {
    pub fn new() -> Self {
        AwaitSet {
            inner: AwaitSetRaw { pending: 0 },
            handles: HashMap::new(),
        }
    }

    pub fn add(&mut self, handle: AsyncCallHandle<T>) {
        self.inner.pending |= 1 << handle.id;
        self.handles.insert(handle.id, handle);
    }

    pub fn del(&mut self, id: usize) {
        self.inner.pending &= !(1 << id);
        self.handles.remove(&id);
    }

    pub fn size(&self) -> u32 {
        self.inner.pending.count_ones()
    }

    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    pub fn await_any(&mut self) -> Option<AsyncCallHandle<T>> {
        if self.is_empty() { return None; }

        let mut completion = LauberhornCompletion::new();
        let res = unsafe {
            lauberhorn_await_any(
                &self.inner as *const AwaitSetRaw,
                &mut completion as *mut LauberhornCompletion,
            )
        };
        if !res { return None; }

        let mut handle = self.handles.remove(&(completion.idx as usize))?;
        self.inner.pending &= !(1 << handle.id);

        handle.data = Some(Box::new(unsafe {
            std::ptr::read(completion.data as *const T)
        }));

        // call lauberhorn free to free the result on it !
        Some(handle)
    }

}


impl LauberhornCompletion {
    pub fn new() -> Self {
        LauberhornCompletion {
            idx: 0,
            data: 0 as *mut c_void
        }
    }
}

pub struct AsyncCallHandle<T: RpcDecode> {
    pub id: usize,
    data: Option<Box<T>>,
}

impl<T: RpcDecode> AsyncCallHandle<T> {
    pub fn new(id: usize) -> Self {
        AsyncCallHandle { id: id, data: None }
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

    if res > 0 {
        Ok(AsyncCallHandle::new(res as usize))
    } else {
        Err(())
    }
}