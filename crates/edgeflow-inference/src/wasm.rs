/// WASM Component executor.
///
/// Loads a WASM component and calls its exported transform function.
///
/// Configurable components (standard Rust pipeline) additionally export
/// `init(list<u8>)`, which is called once with the JSON pipeline config
/// before any `transform` call.  Components that only export `transform`
/// (legacy componentize-py components) remain fully compatible — the host
/// simply skips the init step if the export is absent.
///
/// WASI is provided so componentize-py components (which bundle a Python
/// runtime) can satisfy their system imports.

use anyhow::{Context, Result};
use wasmtime::{
    component::{Component, Func, Linker, Val},
    Config, Engine, Store,
};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

struct State {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

pub struct WasmTransform {
    store: Store<State>,
    func: Func,
}

impl WasmTransform {
    pub fn new(wasm_bytes: &[u8], config: Option<&[u8]>) -> Result<Self> {
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);
        // Disable Cranelift optimizations: 41 MB componentize-py components take
        // several minutes to JIT at O2; None brings that down to seconds.
        cfg.cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = Engine::new(&cfg).context("failed to create wasmtime engine")?;

        let component =
            Component::new(&engine, wasm_bytes).context("failed to compile WASM component")?;

        let mut linker: Linker<State> = Linker::new(&engine);
        wasmtime_wasi::add_to_linker_sync(&mut linker)
            .context("failed to add WASI to component linker")?;

        let state = State {
            ctx: WasiCtxBuilder::new().inherit_stdout().inherit_stderr().build(),
            table: ResourceTable::new(),
        };
        let mut store = Store::new(&engine, state);

        let instance = linker
            .instantiate(&mut store, &component)
            .context("failed to instantiate WASM component")?;

        // Call init if the component exports it (standard Rust pipeline).
        if let Some(cfg_bytes) = config {
            if let Some(init_fn) = instance.get_func(&mut store, "init") {
                let config_val = Val::List(cfg_bytes.iter().map(|&b| Val::U8(b)).collect());
                init_fn
                    .call(&mut store, &[config_val], &mut [])
                    .context("init call failed")?;
                init_fn.post_return(&mut store).ok();
            }
        }

        let func = instance
            .get_func(&mut store, "transform")
            .context("component missing export: transform(list<u8>) -> list<u8>")?;

        Ok(Self { store, func })
    }

    pub fn run(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        let input_val = Val::List(input.iter().map(|&b| Val::U8(b)).collect());
        let mut results = vec![Val::Bool(false)];

        self.func
            .call(&mut self.store, &[input_val], &mut results)
            .context("transform call failed")?;
        self.func
            .post_return(&mut self.store)
            .context("post_return failed")?;

        match results.remove(0) {
            Val::List(items) => {
                let mut bytes = Vec::with_capacity(items.len());
                for v in items {
                    match v {
                        Val::U8(b) => bytes.push(b),
                        other => anyhow::bail!("expected u8 in result list, got {other:?}"),
                    }
                }
                Ok(bytes)
            }
            other => anyhow::bail!("expected list<u8> from transform, got {other:?}"),
        }
    }
}
