use crate::{lauberhorn::marshal::{RpcDecode, RpcEncode}, 
utils::xdr::{xdr_get_opaque, xdr_write_opaque},
webserver::schemas::{DandelionDeserializeResponse, InputSet}};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DandelionArgs {
    pub sets: Vec<InputSet>
}

// User sends XDR encoded request with {function_name, blob}
// where blob is the bjson-encoded dandelion request context
pub struct DandelionRPCRequest {
    pub function_name: String,
    pub payload: DandelionArgs
}

pub struct DandelionRPCResponse {
    // bson encoded output thingy ? 
    pub data: Vec<u8>,
}

impl From<DandelionRPCResponse> for DandelionDeserializeResponse {
    fn from(value: DandelionRPCResponse) -> Self {
        let resp: DandelionDeserializeResponse = bson::from_slice(value.data.as_slice()).unwrap();
        resp
    }
}

impl RpcEncode for DandelionRPCResponse {
    fn rpc_encode(&mut self, out_buf: &mut [u8]) -> Result<usize, ()> {
        // encoding destroys the buffer.... okay for the time being
        xdr_write_opaque(self.data.as_slice(), out_buf.as_ptr() as *mut u8, 1500)
    }
}

impl RpcDecode for DandelionRPCResponse {
    fn rpc_decode(in_buf: &[u8]) -> Result<Self, ()> {
        todo!()
    }
}

impl RpcDecode for DandelionRPCRequest {
    fn rpc_decode(in_buf: &[u8]) -> Result<Self, ()> {

        let (string, rest) = xdr_get_opaque(in_buf).ok_or(())?;
        let (args, _) = xdr_get_opaque(rest).ok_or(())?;

        let payload: DandelionArgs = bson::from_slice(args).map_err(|_| ())?;
        let function_name = String::from_utf8(string.to_vec()).map_err(|_| ())?;

        Ok(DandelionRPCRequest {
            function_name,
            payload,
        })
    }
}

impl RpcEncode for DandelionRPCRequest {
    fn rpc_encode(&self, out_buf: &mut [u8]) -> Result<usize, ()> {
        let args_bson = bson::to_vec(&self.payload).map_err(|_| ())?;
        let func_name = self.function_name.as_bytes();

        let total_size = args_bson.len() + func_name.len();

        if out_buf.len() < total_size {
            return Err(());
        }

        // write name into buffer
        out_buf[0..func_name.len()].copy_from_slice(func_name);
        out_buf[func_name.len()..total_size].copy_from_slice(args_bson.as_slice());

        Ok(total_size)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DandelionNestedResponse {
    pub idx: usize
}

impl RpcDecode for DandelionNestedResponse {
    fn rpc_decode(in_buf: &[u8]) -> Result<Self, ()> {
        let value: DandelionNestedResponse = match bson::from_slice(in_buf) {
            Ok(v) => v,
            Err(_) => return Err(())
        };
        Ok(value)
    }
}

impl RpcEncode for DandelionNestedResponse {
    fn rpc_encode(&self, out_buf: &mut [u8]) -> Result<usize, ()> {
        let bytes = bson::to_vec(self).map_err(|_| ())?; 
        xdr_write_opaque(bytes.as_slice(), out_buf.as_ptr() as *mut u8, 1500)
    }
}
