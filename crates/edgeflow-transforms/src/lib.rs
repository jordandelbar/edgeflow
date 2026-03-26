mod layers;

// WASM target: implement the configurable-transform WIT world
#[cfg(target_arch = "wasm32")]
mod wasm_impl;

// Native target: expose layers to Python via PyO3
#[cfg(feature = "python")]
mod python_impl;
