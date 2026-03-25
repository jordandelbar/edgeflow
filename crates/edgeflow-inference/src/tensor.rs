/// Flat binary tensor format shared between WASM modules and the inference host.
///
/// Layout: [ ndim: u8 | shape: [u32; ndim] (LE) | dtype: u8 | data: [u8] ]
///
/// dtype codes: 1 = f32

pub const DTYPE_F32: u8 = 1;

pub fn encode(shape: &[usize], data: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + shape.len() * 4 + 1 + data.len() * 4);
    buf.push(shape.len() as u8);
    for &dim in shape {
        buf.extend_from_slice(&(dim as u32).to_le_bytes());
    }
    buf.push(DTYPE_F32);
    for &v in data {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

pub fn decode(bytes: &[u8]) -> anyhow::Result<(Vec<usize>, Vec<f32>)> {
    anyhow::ensure!(!bytes.is_empty(), "empty tensor buffer");
    let mut pos = 0;

    let ndim = bytes[pos] as usize;
    pos += 1;

    anyhow::ensure!(bytes.len() >= pos + ndim * 4 + 1, "tensor buffer too short for shape");
    let mut shape = Vec::with_capacity(ndim);
    for _ in 0..ndim {
        let dim = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        shape.push(dim);
        pos += 4;
    }

    let dtype = bytes[pos];
    pos += 1;
    anyhow::ensure!(dtype == DTYPE_F32, "unsupported dtype {dtype}, only f32 (1) is supported");

    let data: Vec<f32> = bytes[pos..]
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();

    Ok((shape, data))
}
