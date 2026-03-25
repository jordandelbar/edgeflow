use anyhow::{Context, Result};
use tract_onnx::prelude::*;

use crate::tensor;
use crate::wasm::WasmTransform;

type TractModel = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

pub struct Pipeline {
    pre: Option<WasmTransform>,
    model: TractModel,
    post: Option<WasmTransform>,
}

impl Pipeline {
    pub fn new(
        model_bytes: &[u8],
        pre_bytes: Option<&[u8]>,
        post_bytes: Option<&[u8]>,
    ) -> Result<Self> {
        tracing::info!("loading ORT session...");
        let model = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(model_bytes))
            .context("failed to parse ONNX model")?
            .with_input_fact(0, tract_onnx::prelude::InferenceFact::dt_shape(
                tract_onnx::prelude::DatumType::F32,
                tract_onnx::prelude::tvec![
                    tract_onnx::prelude::TDim::Val(1),
                    tract_onnx::prelude::TDim::Val(4),
                ],
            ))
            .context("failed to set input fact")?
            .into_optimized()
            .context("failed to optimize ONNX model")?
            .into_runnable()
            .context("failed to make ONNX model runnable")?;
        tracing::info!("ORT session loaded");

        tracing::info!("compiling preprocess WASM...");
        let pre = pre_bytes.map(WasmTransform::new).transpose()?;
        tracing::info!("preprocess WASM ready");

        tracing::info!("compiling postprocess WASM...");
        let post = post_bytes.map(WasmTransform::new).transpose()?;
        tracing::info!("postprocess WASM ready");

        Ok(Self { pre, model, post })
    }

    pub fn infer(&mut self, raw_input: &[u8]) -> Result<Vec<u8>> {
        // Pre-process: raw bytes → tensor bytes
        let tensor_bytes = match &mut self.pre {
            Some(t) => t.run(raw_input)?,
            None => raw_input.to_vec(),
        };

        // Decode flat binary tensor format → f32 data
        let (shape, data) = tensor::decode(&tensor_bytes)?;
        let n: usize = shape.iter().product();
        anyhow::ensure!(data.len() == n, "tensor data length mismatch");

        // Build tract tensor (f32, shape as tract dims)
        let tract_shape: Vec<usize> = shape;
        let input = tract_ndarray::Array::from_shape_vec(tract_shape.as_slice(), data)
            .context("failed to build input array")?
            .into_dyn();
        let input_tensor: Tensor = input.into();

        // Run model
        let outputs = self.model.run(tvec!(input_tensor.into()))
            .context("inference failed")?;

        // Extract first output as f32
        let out = outputs[0]
            .to_array_view::<f32>()
            .context("failed to extract f32 output")?;
        let out_shape: Vec<usize> = out.shape().to_vec();
        let out_data: Vec<f32> = out.iter().copied().collect();

        // Encode back to tensor bytes
        let output_tensor_bytes = tensor::encode(&out_shape, &out_data);

        // Post-process: tensor bytes → raw bytes
        match &mut self.post {
            Some(t) => t.run(&output_tensor_bytes),
            None => Ok(output_tensor_bytes),
        }
    }
}
