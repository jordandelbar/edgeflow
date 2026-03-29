// Wire format implementation lives in edgeflow-common so server and inference
// are guaranteed to speak the same protocol.
pub use edgeflow_common::tensor::{decode, encode};
