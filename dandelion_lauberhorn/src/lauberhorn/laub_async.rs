use std::ffi::c_void;

use crate::lauberhorn::{
    codec::LauberhornRpcEndpoint,
    ffi::{
        lauberhorn_call_async, lauberhorn_future_select, LauberhornFuture, LauberhornRpcEndpointRaw,
        RpcStatus,
    },
    marshal::{RpcDecode, RpcEncode},
};


pub struct AwaitSet<T: RpcDecode, C> {
    handles: Vec<AsyncCallHandle<T, C>>,
}

impl<T: RpcDecode, C> AwaitSet<T, C> {
    pub fn new() -> Self {
        AwaitSet { handles: Vec::new() }
    }

    pub fn add(&mut self, handle: AsyncCallHandle<T, C>) {
        self.handles.push(handle);
    }

    pub fn size(&self) -> u32 {
        self.handles.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    pub fn await_any(&mut self) -> Option<AsyncCallHandle<T, C>> {
        if self.handles.is_empty() {
            return None;
        }

        let mut ptrs: Vec<*mut LauberhornFuture> =
            self.handles.iter_mut().map(|h| h.future_ptr()).collect();

        
        let idx = unsafe { lauberhorn_future_select(ptrs.as_mut_ptr(), ptrs.len() as i32) };
        if idx < 0 {
            return None;
        }

        let mut handle = self.handles.swap_remove(idx as usize);
        let result = unsafe { &mut (*handle.future_ptr()).result };

        match result.code {
            RpcStatus::RpcOk => {
                // claim ownership of the decoded response back
                let value: Box<T> = unsafe { Box::from_raw(result.data as *mut T) };
                handle.data = Some(value);
                result.data = std::ptr::null_mut();
            }
            other => handle.status = other,
        }
        Some(handle)
    }
}

pub struct AsyncCallHandle<T: RpcDecode, C> {
    pub status: RpcStatus,
    pub context: C,
    future: Box<LauberhornFuture>,
    data: Option<Box<T>>,
}

impl<T: RpcDecode, C> AsyncCallHandle<T, C> {
    fn future_ptr(&mut self) -> *mut LauberhornFuture {
        &mut *self.future as *mut LauberhornFuture
    }

    pub fn take_data(&mut self) -> Option<Box<T>> {
        self.data.take()
    }

    pub fn is_complete(&self) -> bool {
        self.data.is_some()
    }
}

pub fn call_async<Req: RpcEncode, Resp: RpcDecode, C>(
    ep: &LauberhornRpcEndpoint<Req, Resp>,
    payload: &Req,
    context: C,
) -> Result<AsyncCallHandle<Resp, C>, ()> {
    let mut future = Box::new(LauberhornFuture::empty());

    let res = unsafe {
        lauberhorn_call_async(
            &ep.inner as *const LauberhornRpcEndpointRaw,
            payload as *const Req as *const c_void,
            std::mem::size_of::<Req>(),
            &mut *future as *mut LauberhornFuture,
        )
    };

    // 0 on success, -1 if the pending-call table is full.
    if res != 0 {
        Err(())
    } else {
        Ok(AsyncCallHandle {
            status: RpcStatus::RpcOk,
            context,
            future,
            data: None,
        })
    }
}

// call_cb not implemented as not needed
