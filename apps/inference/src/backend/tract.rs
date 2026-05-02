use anyhow::{Context, Result};
use tract_onnx::prelude::*;
use tract_onnx::tract_hir::infer::Factoid;
use tract_onnx::tract_hir::internal::DimLike;

use super::InferenceBackend;
use edgeflow_common::tensor;

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
        let parsed = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(model_bytes))
            .context("failed to parse ONNX model")?;

        // Derive a concrete input shape from the ONNX graph.
        // Dynamic dims (e.g. batch=None) are concretised to 1 - the
        // inference pod always runs single-sample requests.
        let input_shape: Vec<usize> = {
            let fact = parsed.input_fact(0).context("model has no inputs")?;
            let rank = fact
                .shape
                .rank()
                .concretize()
                .map(|r| r as usize)
                .unwrap_or(2);
            (0..rank)
                .map(|i| {
                    fact.shape
                        .dim(i)
                        .and_then(|d| d.concretize())
                        .and_then(|tdim: TDim| tdim.to_usize().ok())
                        .unwrap_or(1)
                })
                .collect()
        };

        let model = parsed
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), input_shape))
            .context("failed to set input fact")?
            .into_optimized()
            .context("failed to optimize ONNX model")?
            .into_runnable()
            .context("failed to make ONNX model runnable")?;
        self.model = Some(model);
        Ok(())
    }

    fn infer(&mut self, shape: &[usize], data: &[f32], out: &mut Vec<u8>) -> Result<()> {
        let model = self.model.as_ref().context("model not loaded")?;

        let input = tract_ndarray::Array::from_shape_vec(shape, data.to_vec())
            .context("failed to build input array")?
            .into_dyn();
        let input_tensor: Tensor = input.into();

        let outputs = model
            .run(tvec!(input_tensor.into()))
            .context("tract inference failed")?;

        let view = outputs[0]
            .to_array_view::<f32>()
            .context("failed to extract f32 output from tract")?;

        let out_shape = view.shape().to_vec();
        let slice = view.as_slice().context("tract output is non-contiguous")?;
        tensor::encode_into(&out_shape, slice, out);
        Ok(())
    }
}
