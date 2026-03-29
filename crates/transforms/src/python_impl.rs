use pyo3::prelude::*;

use crate::layers::{run_pipeline, PipelineConfig};

/// Native Rust pipeline executor, exposed to Python via PyO3.
///
/// Python layer classes serialise themselves to a JSON config; this class
/// deserialises that config and runs the same Rust logic that executes
/// inside the WASM component on the server — local results are guaranteed
/// to match server results.
#[pyclass]
pub struct NativePipeline {
    config: PipelineConfig,
}

#[pymethods]
impl NativePipeline {
    #[new]
    fn new(config_json: &[u8]) -> PyResult<Self> {
        let config = serde_json::from_slice(config_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { config })
    }

    fn transform(&self, input: &[u8]) -> Vec<u8> {
        run_pipeline(&self.config, input.to_vec())
    }
}

#[pymodule]
fn _lib(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NativePipeline>()?;
    Ok(())
}
