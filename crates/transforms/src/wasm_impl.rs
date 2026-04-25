use std::sync::OnceLock;

use crate::layers::{run_pipeline, run_pipeline_from, PipelineConfig};

wit_bindgen::generate!({
    world: "configurable-transform",
    path: "wit",
});

struct Component;

static CONFIG: OnceLock<PipelineConfig> = OnceLock::new();

impl Guest for Component {
    fn init(config: Vec<u8>) {
        let cfg = serde_json::from_slice(&config).expect("invalid pipeline config JSON");
        CONFIG.set(cfg).expect("init called twice");
    }

    fn transform(input: Vec<u8>) -> Vec<u8> {
        let cfg = CONFIG.get().expect("init must be called before transform");
        run_pipeline(cfg, input)
    }

    fn transform_from(input: Vec<u8>, start: u32) -> Vec<u8> {
        let cfg = CONFIG
            .get()
            .expect("init must be called before transform-from");
        run_pipeline_from(cfg, input, start as usize)
    }
}

export!(Component);
