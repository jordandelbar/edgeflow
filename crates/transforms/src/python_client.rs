//! PyO3 bindings to `edgeflow_client::Api`.
//!
//! The Python SDK and the `edgeflow` CLI share this client by design:
//! both call the same Rust methods, so retries, error mapping, and the
//! request shape live in exactly one place. The Python side gets a thin
//! `Client` class plus typed module-level helpers.
//!
//! Return values are converted to plain Python objects (dict / list /
//! str / number / None) via `pythonize`. The user-facing typed Python
//! classes (e.g. `ModelVersion`) live in `apps/sdk/edgeflow/` and wrap
//! these dicts.

use anyhow::Result as AnyhowResult;
use edgeflow_client::Api;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Map any `anyhow::Error` from the Rust client to a Python exception.
fn to_py_err(e: anyhow::Error) -> PyErr {
    PyRuntimeError::new_err(format!("{e:#}"))
}

fn unwrap<T>(r: AnyhowResult<T>) -> PyResult<T> {
    r.map_err(to_py_err)
}

/// Convert a `serde_json::Value` to a Python object via pythonize.
fn json_to_py<'py>(py: Python<'py>, value: serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    pythonize::pythonize(py, &value).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Sync HTTP client for the edgeflow REST API.
///
/// Construct once with the server URL, then reuse across calls. Methods
/// return plain Python objects (dict / list / str / None) - the user
/// applies their own typed wrappers on top.
#[pyclass]
pub struct Client {
    inner: Api,
}

#[pymethods]
impl Client {
    #[new]
    fn new(server: &str) -> Self {
        Self {
            inner: Api::new(server),
        }
    }

    // ── Experiments ───────────────────────────────────────────────────────────

    fn list_experiments<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.list_experiments())?)
    }

    fn get_experiment<'py>(&self, py: Python<'py>, id: &str) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.get_experiment(id))?)
    }

    fn resolve_experiment<'py>(
        &self,
        py: Python<'py>,
        name_or_id: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.resolve_experiment(name_or_id))?)
    }

    // ── Runs ──────────────────────────────────────────────────────────────────

    fn search_runs<'py>(
        &self,
        py: Python<'py>,
        experiment_id: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.search_runs(experiment_id))?)
    }

    fn get_run<'py>(&self, py: Python<'py>, run_id: &str) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.get_run(run_id))?)
    }

    fn resolve_run_id(&self, prefix: &str) -> PyResult<String> {
        unwrap(self.inner.resolve_run_id(prefix))
    }

    // ── Model Registry ────────────────────────────────────────────────────────

    fn list_registered_models<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.list_registered_models())?)
    }

    fn list_model_versions<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.list_model_versions(name))?)
    }

    fn register_model<'py>(
        &self,
        py: Python<'py>,
        run_id: &str,
        name: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.register_model(run_id, name))?)
    }

    fn transition_stage<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        version: &str,
        stage: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(
            py,
            unwrap(self.inner.transition_stage(name, version, stage))?,
        )
    }

    fn delete_registered_model<'py>(
        &self,
        py: Python<'py>,
        name: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.delete_registered_model(name))?)
    }

    fn delete_model_version<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        version: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.delete_model_version(name, version))?)
    }

    // ── Deployments ───────────────────────────────────────────────────────────

    #[pyo3(signature = (model_name, model_version, target, sessions=None, max_concurrent=None))]
    fn create_deployment<'py>(
        &self,
        py: Python<'py>,
        model_name: &str,
        model_version: &str,
        target: &str,
        sessions: Option<i64>,
        max_concurrent: Option<i64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(
            py,
            unwrap(self.inner.create_deployment(
                model_name,
                model_version,
                target,
                sessions,
                max_concurrent,
            ))?,
        )
    }

    #[pyo3(signature = (target=None))]
    fn list_deployments<'py>(
        &self,
        py: Python<'py>,
        target: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.list_deployments(target))?)
    }

    fn get_deployment<'py>(&self, py: Python<'py>, id: &str) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.get_deployment(id))?)
    }

    fn latest_deployment<'py>(&self, py: Python<'py>, target: &str) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.latest_deployment(target))?)
    }

    // ── Targets ───────────────────────────────────────────────────────────────

    fn list_targets<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.list_targets())?)
    }

    fn get_target<'py>(&self, py: Python<'py>, target: &str) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.get_target(target))?)
    }

    #[pyo3(signature = (target, sessions=None, max_concurrent=None, cpu_request=None, memory_request=None, memory_limit=None, replicas=None, placement=None))]
    #[allow(clippy::too_many_arguments)]
    fn update_target_resources<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        sessions: Option<i64>,
        max_concurrent: Option<i64>,
        cpu_request: Option<&str>,
        memory_request: Option<&str>,
        memory_limit: Option<&str>,
        replicas: Option<i64>,
        placement: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(
            py,
            unwrap(self.inner.update_target_resources(
                target,
                sessions,
                max_concurrent,
                cpu_request,
                memory_request,
                memory_limit,
                replicas,
                placement,
            ))?,
        )
    }

    fn teardown_target(&self, target: &str) -> PyResult<()> {
        unwrap(self.inner.teardown_target(target))
    }

    // ── Nodes ─────────────────────────────────────────────────────────────────

    fn list_nodes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        json_to_py(py, unwrap(self.inner.list_nodes())?)
    }
}
