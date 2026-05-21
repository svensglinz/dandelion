fn xdr_pad_len(len: usize) -> usize {
    return (len + 3) & !3;
}

pub fn xdr_get_opaque(data: &[u8]) -> Option<(&[u8], &[u8])> {
    if data.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
    let padded = xdr_pad_len(len);
    if data.len() < 4 + padded {
        return None;
    }
    // (value, rest)
    Some((&data[4..4 + len], &data[4 + padded..]))
}

pub fn xdr_write_opaque(src: &[u8], dst: *mut u8, dst_len: usize) -> Result<usize, ()> {
    // length check 
    let padded = xdr_pad_len(src.len());
    let total = 4 + padded; 
    if dst_len < total {
        return Err(()); 
    }

    unsafe {
        let len_bytes = (src.len() as u32).to_be_bytes();
        // write be size 
        std::ptr::copy_nonoverlapping(len_bytes.as_ptr(), dst, 4);
        // write data
        std::ptr::copy_nonoverlapping(src.as_ptr(), dst.add(4), src.len());
    };
    Some(total)
}
