use anyhow::{Context, Result};
use tract_onnx::prelude::*;

use super::InferenceBackend;

type TractModel = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

pub struct TractBackend {
    model: Option<TractModel>,
}

impl TractBackend {
    pub fn new() -> Self {
        Self { model: None }
    }
}

impl InferenceBackend for TractBackend {
    fn load(&mut self, model_bytes: &[u8]) -> Result<()> {
        let model = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(model_bytes))
            .context("failed to parse ONNX model")?
            .with_input_fact(0, InferenceFact::default())
            .context("failed to set input fact")?
            .into_optimized()
            .context("failed to optimize ONNX model")?
            .into_runnable()
            .context("failed to make ONNX model runnable")?;
        self.model = Some(model);
        Ok(())
    }

    fn infer(&mut self, shape: &[usize], data: &[f32]) -> Result<(Vec<usize>, Vec<f32>)> {
        let model = self.model.as_ref().context("model not loaded")?;

        let input = tract_ndarray::Array::from_shape_vec(shape, data.to_vec())
            .context("failed to build input array")?
            .into_dyn();
        let input_tensor: Tensor = input.into();

        let outputs = model
            .run(tvec!(input_tensor.into()))
            .context("tract inference failed")?;

        let out = outputs[0]
            .to_array_view::<f32>()
            .context("failed to extract f32 output from tract")?;

        let out_shape = out.shape().to_vec();
        let out_data = out.iter().copied().collect();

        Ok((out_shape, out_data))
    }
}
