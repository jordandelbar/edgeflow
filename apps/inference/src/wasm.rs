/// WASM Component executor.
///
/// Loads a WASM component built against the `configurable-transform` WIT world
/// and calls its exported transform function. Static host-side bindings come
/// from `wasmtime::component::bindgen!()`, which lowers `list<u8>` boundary
/// crossings to a single memcpy per direction.
use anyhow::{Context, Result};
use wasmtime::{
    component::{Component, Linker},
    Config, Engine, Store,
};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

mod bindings {
    wasmtime::component::bindgen!({
        world: "configurable-transform",
        path: "../../crates/transforms/wit/transform.wit",
    });
}

struct State {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

pub struct WasmTransform {
    store: Store<State>,
    component: bindings::ConfigurableTransform,
}

impl WasmTransform {
    /// Create a wasmtime Engine configured for WASM component use.
    ///
    /// Cranelift optimisations are disabled - large components took several
    /// minutes to JIT at O2; None brings that down to seconds. Callers building
    /// multiple transforms should call this once and pass the result to each
    /// `WasmTransform::new` to share JIT resources.
    pub fn build_engine() -> Result<Engine> {
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);
        cfg.cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&cfg)
            .map_err(anyhow::Error::from)
            .context("failed to create wasmtime engine")
    }

    /// Build a transform from raw WASM bytes plus its config.
    pub fn new(engine: &Engine, wasm_bytes: &[u8], config: &[u8]) -> Result<Self> {
        let component = Component::new(engine, wasm_bytes)
            .map_err(anyhow::Error::from)
            .context("failed to compile WASM component")?;

        let mut linker: Linker<State> = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(anyhow::Error::from)
            .context("failed to add WASI to component linker")?;

        let state = State {
            ctx: WasiCtxBuilder::new()
                .inherit_stdout()
                .inherit_stderr()
                .build(),
            table: ResourceTable::new(),
        };
        let mut store = Store::new(engine, state);

        let component_bindings =
            bindings::ConfigurableTransform::instantiate(&mut store, &component, &linker)
                .map_err(anyhow::Error::from)
                .context("failed to instantiate WASM component")?;

        component_bindings
            .call_init(&mut store, config)
            .map_err(anyhow::Error::from)
            .context("init call failed")?;

        Ok(Self {
            store,
            component: component_bindings,
        })
    }

    pub fn run(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        self.run_from(input, 0)
    }

    /// Run the transform starting at step `start`. `start = 0` is equivalent to
    /// `run`. Used by the host to skip a leading format-adapter step when the
    /// input already arrived as a tensor (JSON-array path), without having to
    /// instantiate a second WASM component.
    pub fn run_from(&mut self, input: &[u8], start: u32) -> Result<Vec<u8>> {
        self.component
            .call_transform_from(&mut self.store, input, start)
            .map_err(anyhow::Error::from)
            .context("transform call failed")
    }
}
