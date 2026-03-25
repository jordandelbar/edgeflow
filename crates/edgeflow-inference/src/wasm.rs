/// WASM Component executor.
///
/// Loads a WASM component produced by componentize-py and calls its exported
/// `transform(list<u8>) -> list<u8>` function (defined in wit/transform.wit).
///
/// WASI is provided so componentize-py components (which bundle a Python runtime)
/// can satisfy their system imports. The components themselves are sandboxed:
/// no filesystem, no network, no env — only the transform call surface.

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
    pub fn new(wasm_bytes: &[u8]) -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        // Disable Cranelift optimizations: 41 MB componentize-py components take
        // several minutes to JIT at O2; None brings that down to seconds.
        config.cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = Engine::new(&config).context("failed to create wasmtime engine")?;

        let component =
            Component::new(&engine, wasm_bytes).context("failed to compile WASM component")?;

        let mut linker: Linker<State> = Linker::new(&engine);
        wasmtime_wasi::add_to_linker_sync(&mut linker)
            .context("failed to add WASI to component linker")?;

        let state = State {
            // Inherit stdout/stderr so Python exceptions are visible in logs.
            ctx: WasiCtxBuilder::new().inherit_stdout().inherit_stderr().build(),
            table: ResourceTable::new(),
        };
        let mut store = Store::new(&engine, state);

        let instance = linker
            .instantiate(&mut store, &component)
            .context("failed to instantiate WASM component")?;

        let func = instance
            .get_func(&mut store, "transform")
            .context("component missing export: transform(list<u8>) -> list<u8>")?;

        Ok(Self { store, func })
    }

    pub fn run(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        let input_val = Val::List(input.iter().map(|&b| Val::U8(b)).collect());
        let mut results = vec![Val::Bool(false)]; // placeholder, overwritten by call

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
