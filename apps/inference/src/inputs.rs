/// Schema-driven JSON input parsing and categorical encoding.
///
/// When an ONNX model is deployed with a `schema.json` whose `input.format`
/// is `"json"`, the inference endpoint accepts a JSON object instead of raw
/// bytes.  This module:
///
///   1. Parses `schema.json` to extract the field list and any encoding tables.
///   2. At request time, converts the JSON body to a flat f32 tensor using those
///      tables, which is then fed to the regular f32 inference path.
///
/// # Encoding strategy
///
/// Rather than pushing string inputs through ORT (which requires the unstable
/// ORT string-tensor API), the sklearn preprocessor's fitted state is exported
/// to `schema.json` at training time.  Encoding is a pure deterministic lookup
/// (no algorithmic re-implementation), so skew risk is negligible.
///
/// Supported encodings (matching sklearn transform types):
///   - `ordinal`: category → f32 ordinal index (OrdinalEncoder)
///   - `one_hot`: category → binary vector of length `|categories|` (OneHotEncoder)
///   - `passthrough`: numeric field passed through unchanged
use std::collections::HashMap;

use anyhow::{anyhow, Result};
use serde::Deserialize;

// ── schema types ────────────────────────────────────────────────────────────

/// How the deployed model expects its HTTP input.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    /// Raw `n_features × f32 LE` bytes - backward-compatible wire protocol.
    Single,
    /// JSON object with named fields - used for mixed-type tabular models.
    Named,
}

#[derive(Debug, Clone)]
pub enum Encoding {
    /// OrdinalEncoder: string → f32 index.
    Ordinal(HashMap<String, f32>),
    /// OneHotEncoder: string → one binary f32 per category (in order).
    OneHot { categories: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct InputSpec {
    pub name: String,
    pub encoding: Option<Encoding>,
}

// ── schema.json deserialization ──────────────────────────────────────────────

#[derive(Deserialize)]
struct SchemaJson {
    input: Option<InputSection>,
}

#[derive(Deserialize)]
struct InputSection {
    format: String,
    fields: Option<Vec<FieldJson>>,
}

#[derive(Deserialize)]
struct FieldJson {
    name: String,
    // Kept for schema completeness; encoding presence implies a string type.
    #[allow(dead_code)]
    #[serde(rename = "type")]
    dtype: String,
    encoding: Option<EncodingJson>,
}

#[derive(Deserialize)]
struct EncodingJson {
    #[serde(rename = "type")]
    kind: String,
    /// OrdinalEncoder: {"Private": 0.0, "Self-emp": 1.0, ...}
    map: Option<HashMap<String, f32>>,
    /// OneHotEncoder: ["Divorced", "Married-civ-spouse", ...]
    categories: Option<Vec<String>>,
}

// ── public API ───────────────────────────────────────────────────────────────

/// Parse `schema.json` bytes and return the input mode plus field specs.
///
/// Falls back to `(Single, [])` for missing schemas, unknown formats, or
/// parse errors - preserving backward compatibility with float-bytes models.
pub fn parse_schema(bytes: Option<&[u8]>) -> (InputMode, Vec<InputSpec>) {
    let bytes = match bytes {
        Some(b) if !b.is_empty() => b,
        _ => return (InputMode::Single, vec![]),
    };

    let schema: SchemaJson = match serde_json::from_slice(bytes) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("failed to parse schema.json, falling back to Single mode: {e}");
            return (InputMode::Single, vec![]);
        }
    };

    let input = match schema.input {
        Some(i) => i,
        None => return (InputMode::Single, vec![]),
    };

    match input.format.as_str() {
        "json" => {}
        "image" => {
            tracing::info!("image input mode: raw JPEG/PNG bytes routed through preprocess WASM");
            return (InputMode::Single, vec![]);
        }
        other => {
            tracing::info!(
                format = other,
                "unrecognised input format, using Single mode"
            );
            return (InputMode::Single, vec![]);
        }
    }

    let fields = match input.fields {
        Some(f) if !f.is_empty() => f,
        _ => {
            tracing::warn!(
                "schema.json has format=json but no fields, falling back to Single mode"
            );
            return (InputMode::Single, vec![]);
        }
    };

    let specs: Vec<InputSpec> = fields
        .into_iter()
        .map(|f| {
            let encoding = f.encoding.map(|enc| match enc.kind.as_str() {
                "ordinal" => Encoding::Ordinal(enc.map.unwrap_or_default()),
                "one_hot" => Encoding::OneHot {
                    categories: enc.categories.unwrap_or_default(),
                },
                other => {
                    tracing::warn!("unknown encoding type '{other}', treating as passthrough");
                    Encoding::Ordinal(HashMap::new())
                }
            });
            InputSpec {
                name: f.name,
                encoding,
            }
        })
        .collect();

    tracing::info!(fields = specs.len(), "loaded Named-mode schema");
    (InputMode::Named, specs)
}

/// True if `body`'s first non-whitespace byte is `[`.
///
/// Used by the inference pipeline to dispatch a `Single`-mode body to the
/// JSON-array decoder instead of the binary tensor decoder, with no schema
/// or content-type plumbing.
pub fn looks_like_json_array(body: &[u8]) -> bool {
    body.iter()
        .find(|&&b| !b.is_ascii_whitespace())
        .is_some_and(|&b| b == b'[')
}

/// Decode a nested JSON array of numbers into a flat f32 tensor.
///
/// Shape is inferred from nesting depth. A 1D body (e.g. `[5.1, 3.5, 1.4, 0.2]`)
/// is auto-batched to `[1, n]`; multi-dim bodies (`[[..],[..]]`) keep their
/// natural shape. Ragged sub-arrays and non-numeric elements are rejected.
pub fn json_array_to_tensor(body: &[u8]) -> Result<(Vec<usize>, Vec<f32>)> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| anyhow!("invalid JSON body: {e}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| anyhow!("expected a JSON array"))?;
    if arr.is_empty() {
        return Err(anyhow!("input array must not be empty"));
    }

    let mut shape: Vec<usize> = Vec::new();
    let mut data: Vec<f32> = Vec::new();
    walk_json_array(&value, &mut shape, &mut data, 0)?;

    // Auto-batch 1D inputs so the common `[a, b, c, d]` curl matches the
    // typical `[1, n_features]` model input shape.
    if shape.len() == 1 {
        shape.insert(0, 1);
    }
    Ok((shape, data))
}

fn walk_json_array(
    value: &serde_json::Value,
    shape: &mut Vec<usize>,
    data: &mut Vec<f32>,
    depth: usize,
) -> Result<()> {
    match value {
        serde_json::Value::Array(items) => {
            if depth == shape.len() {
                shape.push(items.len());
            } else if shape[depth] != items.len() {
                return Err(anyhow!(
                    "ragged array: expected {} elements at depth {depth}, got {}",
                    shape[depth],
                    items.len()
                ));
            }
            for item in items {
                walk_json_array(item, shape, data, depth + 1)?;
            }
            Ok(())
        }
        serde_json::Value::Number(n) => {
            let f = n
                .as_f64()
                .ok_or_else(|| anyhow!("number out of range: {n}"))?;
            data.push(f as f32);
            Ok(())
        }
        other => Err(anyhow!("expected number, got {}", other)),
    }
}

/// Convert a JSON request body to a flat `[1, n_features]` f32 tensor.
///
/// Field order follows `specs` exactly - this must match the column order
/// expected by the deployed ONNX model.
pub fn json_to_tensor(body: &[u8], specs: &[InputSpec]) -> Result<(Vec<usize>, Vec<f32>)> {
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(body).map_err(|e| anyhow!("invalid JSON body: {e}"))?;

    let capacity: usize = specs
        .iter()
        .map(|s| match &s.encoding {
            Some(Encoding::OneHot { categories }) => categories.len(),
            _ => 1,
        })
        .sum();
    let mut features: Vec<f32> = Vec::with_capacity(capacity);

    for spec in specs {
        let raw = obj
            .get(&spec.name)
            .ok_or_else(|| anyhow!("missing required field '{}'", spec.name))?;

        match &spec.encoding {
            None => {
                // Numeric passthrough.
                let f = raw
                    .as_f64()
                    .ok_or_else(|| anyhow!("field '{}' must be a number", spec.name))?;
                features.push(f as f32);
            }

            Some(Encoding::Ordinal(map)) => {
                let s = raw
                    .as_str()
                    .ok_or_else(|| anyhow!("field '{}' must be a string", spec.name))?;
                let &idx = map
                    .get(s)
                    .ok_or_else(|| anyhow!("unknown category '{}' for field '{}'", s, spec.name))?;
                features.push(idx);
            }

            Some(Encoding::OneHot { categories }) => {
                let s = raw
                    .as_str()
                    .ok_or_else(|| anyhow!("field '{}' must be a string", spec.name))?;
                for cat in categories {
                    features.push(if cat == s { 1.0 } else { 0.0 });
                }
            }
        }
    }

    let n = features.len();
    Ok((vec![1, n], features))
}
