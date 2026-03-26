use serde::{Deserialize, Serialize};

// Wire format: [ ndim: u8 | shape: [u32-LE; ndim] | dtype: u8 (1=f32) | data: bytes ]

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layer {
    FloatToTensor { n_features: usize },
    Normalize { mean: Vec<f32>, std: Vec<f32> },
    ClassifierOutput { labels: Vec<String> },
    RawTensorOutput,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PipelineConfig {
    pub steps: Vec<Layer>,
}

pub fn run_pipeline(config: &PipelineConfig, input: Vec<u8>) -> Vec<u8> {
    let mut data = input;
    for step in &config.steps {
        data = run_layer(step, data);
    }
    data
}

fn run_layer(layer: &Layer, input: Vec<u8>) -> Vec<u8> {
    match layer {
        Layer::FloatToTensor { n_features } => float_to_tensor(*n_features, &input),
        Layer::Normalize { mean, std } => normalize(mean, std, &input),
        Layer::ClassifierOutput { labels } => classifier_output(labels, &input),
        Layer::RawTensorOutput => input,
    }
}

fn encode_tensor(shape: &[usize], data: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + shape.len() * 4 + 1 + data.len() * 4);
    buf.push(shape.len() as u8);
    for &dim in shape {
        buf.extend_from_slice(&(dim as u32).to_le_bytes());
    }
    buf.push(1u8); // dtype = f32
    for &v in data {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

fn decode_tensor(buf: &[u8]) -> (Vec<usize>, Vec<f32>) {
    let mut pos = 0;
    let ndim = buf[pos] as usize;
    pos += 1;
    let mut shape = Vec::with_capacity(ndim);
    for _ in 0..ndim {
        let dim = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
        shape.push(dim);
        pos += 4;
    }
    pos += 1; // skip dtype
    let values = buf[pos..]
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    (shape, values)
}

fn float_to_tensor(n_features: usize, raw: &[u8]) -> Vec<u8> {
    let data: Vec<f32> = raw
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    encode_tensor(&[1, n_features], &data)
}

fn normalize(mean: &[f32], std: &[f32], input: &[u8]) -> Vec<u8> {
    let (shape, data) = decode_tensor(input);
    let n = mean.len();
    let normalized: Vec<f32> = data
        .iter()
        .enumerate()
        .map(|(i, &v)| (v - mean[i % n]) / std[i % n])
        .collect();
    encode_tensor(&shape, &normalized)
}

fn classifier_output(labels: &[String], input: &[u8]) -> Vec<u8> {
    let (_, probs) = decode_tensor(input);
    let (class_id, &confidence) = probs
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0, &0.0));
    let label = labels.get(class_id).map(|s| s.as_str()).unwrap_or("unknown");
    let confidence = (confidence * 10000.0).round() / 10000.0;
    format!(r#"{{"class_id":{class_id},"label":"{label}","confidence":{confidence}}}"#)
        .into_bytes()
}
