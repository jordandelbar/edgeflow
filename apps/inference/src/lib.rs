// Core inference logic, exported so tests/ and benches/ can import them.
// Server, client, and deployment wiring stay private in main.rs.
pub mod backend;
pub mod inputs;
pub mod pipeline;
pub mod tensor;
pub mod wasm;
