/// Flat binary tensor format shared between the inference host and server.
///
/// Layout: `[ ndim: u8 | dtype: u8 | _pad: u16 | shape: [u32; ndim] (LE) | data: [u8] ]`
///
/// The fixed 4-byte header ensures `data` always starts at offset `4 + ndim*4`,
/// which is a multiple of 4.  Combined with allocators returning ≥8-byte aligned
/// memory, this guarantees f32 alignment for zero-copy decode via bytemuck.
///
/// dtype codes: 1 = f32
pub const DTYPE_F32: u8 = 1;

/// Encode into an existing buffer, reusing capacity. Clears `buf` first.
pub fn encode_into(shape: &[usize], data: &[f32], buf: &mut Vec<u8>) {
    buf.clear();
    buf.reserve(4 + shape.len() * 4 + data.len() * 4);
    buf.push(shape.len() as u8);
    buf.push(DTYPE_F32);
    buf.push(0u8);
    buf.push(0u8);
    for &dim in shape {
        buf.extend_from_slice(&(dim as u32).to_le_bytes());
    }
    buf.extend_from_slice(bytemuck::cast_slice(data));
}

pub fn decode(bytes: &[u8]) -> anyhow::Result<(Vec<usize>, &[f32])> {
    anyhow::ensure!(bytes.len() >= 4, "tensor buffer too short");

    let ndim = bytes[0] as usize;
    let dtype = bytes[1];
    // bytes[2..4] is padding
    let data_offset = 4 + ndim * 4;

    anyhow::ensure!(
        bytes.len() >= data_offset,
        "tensor buffer too short for shape"
    );
    anyhow::ensure!(
        dtype == DTYPE_F32,
        "unsupported dtype {dtype}, only f32 (1) is supported"
    );

    let mut shape = Vec::with_capacity(ndim);
    for i in 0..ndim {
        let off = 4 + i * 4;
        let dim = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
        shape.push(dim);
    }

    let data: &[f32] = bytemuck::try_cast_slice(&bytes[data_offset..])
        .map_err(|e| anyhow::anyhow!("tensor data alignment error: {e:?}"))?;

    Ok((shape, data))
}
