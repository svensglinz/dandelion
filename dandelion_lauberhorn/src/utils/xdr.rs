use std::ffi::{c_char, c_uint, c_void}; 

#[repr(i32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum XdrOp {
    Encode = 0,
    Decode = 1,
    Free = 2,
}

/// Struct representing an XDR stream
/// Equivalemnt to XDR in C
#[repr(C)]
pub struct XdrStream {
    x_op: i32,            // enum xdr_op
    x_ops: *const c_void, // xdr_ops vtable pointer
    x_public: *mut c_char,    // users' data
    x_private: *mut c_void,   // current position in buffer
    x_base: *mut c_char,      // start of buffer
    x_handy: u32,         // remaining bytes
}

impl XdrStream {

    pub fn new(op: XdrOp, buffer: &mut [u8]) -> Self {

        let mut stream = XdrStream {
            x_op: op as i32,
            x_ops: std::ptr::null(), 
            x_public: std::ptr::null_mut(),
            x_private: buffer.as_mut_ptr() as *mut c_void,
            x_base: buffer.as_mut_ptr() as *mut c_char,
            x_handy: buffer.len() as u32,
        };

        unsafe { 
            xdrmem_create(
            &mut stream,
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len() as c_uint, op as i32) 
        };
        stream
    }

     /// Linearize the data in the XDR stream into a Rust Vec<u8>.
    pub fn op(&self) -> Option<XdrOp> {
        match self.x_op {
            0 => Some(XdrOp::Encode),
            1 => Some(XdrOp::Decode),
            2 => Some(XdrOp::Free),
            _ => None,
        }
    }

    /// check if the stream was constructed for encoding
    /// (ie. self.x_op == XdrOp::Encode)
    pub fn is_encode(&self) -> bool {
        self.op() == Some(XdrOp::Encode)
    }

    /// check if the stream was constructed for decoding
    /// (ie. self.x_op == XdrOp::Decode)
    pub fn is_decode(&self) -> bool {
        self.op() == Some(XdrOp::Decode)
    }

    /// returns the remaining bytes to decode
    /// if `self.is_decode() == true`, else 0 -> CHECK
    pub fn get_remaining(&self) -> u32 {
        self.x_handy
    }

    ///
    pub fn get_current_position(&self) -> *mut u8 {
        self.x_private as *mut u8
    }

    /// returns the size of the binary stream to
    /// be decoded if `self.is_decode() == true` else 0
    pub fn size(&self) -> usize {
        (self.x_private as usize) - (self.x_base as usize)
    }

    /// If XDR is in ENCODE mode, returns a slice to the encoded data in the stream
    /// else, returns an empty slice
    pub fn get_data(&self) -> &[u8] {
        let size = self.size();
        let data_ptr = self.x_base as *const u8;
        unsafe { std::slice::from_raw_parts(data_ptr, size) }
        }

     /// Linearize the data in the XDR stream into a Rust Vec<u8>.

    pub fn to_vec(&self) -> Vec<u8> {
        self.get_data().to_vec()
    }

    // Getters
    pub fn get_int(&mut self) -> Option<u32> {
        debug_assert!(self.is_decode(), "get_int called on non-DECODE stream");
        if !self.is_decode() {
            return None;
        }

        let mut value: c_uint = 0;
        let result = unsafe { xdr_u_int(self, &mut value) };
        if result == 0 {
            None
        } else {
            Some(value)
        }
    }

    pub fn get_string(&mut self) -> Option<String> {
        if let Some(length) = self.get_int() {
            let mut buffer = vec![0u8; length as usize];
            let result = unsafe {
                xdr_opaque(
                    self,
                    buffer.as_mut_ptr() as *mut c_char,
                    length,
                )
            };
            if result == 0 {
                None
            } else {
                String::from_utf8(buffer).ok()
            }
        } else {
            None
        }
    }

    // also have a version that just returns length and a pointer to the data in the stream ? 
    pub fn get_opaque(&mut self) -> Option<Vec<u8>> {
        if let Some(length) = self.get_int() {
            let mut buffer = vec![0u8; length as usize];
            let result = unsafe {
                xdr_opaque(
                    self,
                    buffer.as_mut_ptr() as *mut c_char,
                    length,
                )
            };
            if result == 0 {
                None
            } else {
                Some(buffer)
            }
        } else {
            None
        }
    }

    /// append a value of type `u32` to the XDR stream
    /// IMPORTANT: Operation is only valid on encoding streams
    /// (ie. `self.is_encode() == true`)
    pub fn set_uint(&mut self, value: u32) -> bool {
        debug_assert!(self.is_encode(), "set_uint called on non-ENCODE stream");
        if !self.is_encode() {
            return false;
        }

        let mut out = value as c_uint;
        unsafe { xdr_u_int(self, &mut out) != 0 }
    }

    ///
    /// 
    pub fn set_opaque(&mut self, data: &[u8]) -> bool {
        debug_assert!(self.is_encode(), "set_opaque called on non-ENCODE stream");
        if !self.is_encode() {
            return false;
        }
        if data.len() > u32::MAX as usize {
            return false;
        }
        if !self.set_uint(data.len() as u32) {
            return false;
        }

        unsafe { xdr_opaque(self, data.as_ptr() as *mut c_char, data.len() as c_uint) != 0 }
    }

    ///
    /// 
    pub fn set_string(&mut self, value: &str) -> bool {
        self.set_opaque(value.as_bytes())
    }
}


// return 1 on success, 0 on failure
#[link(name = "tirpc")]
unsafe extern "C" {
    fn xdr_u_int(xdrs: *mut XdrStream, up: *mut c_uint) -> i32;
    fn xdr_opaque(xdrs: *mut XdrStream, cp: *mut c_char, cnt: c_uint) -> i32;
    fn xdrmem_create(xdrs: *mut XdrStream, buf: *mut c_char, len: c_uint, op: i32);
}
