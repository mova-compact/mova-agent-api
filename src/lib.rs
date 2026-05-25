//! MOVA Agent API V0 library skeleton.
//!
//! This crate defines module boundaries only in Phase 1.

pub mod connectors;
pub mod contracts;
pub mod auth;
pub mod evidence;
pub mod execution;
pub mod github_file_bridge;
#[cfg(not(target_arch = "wasm32"))]
pub mod http;
pub mod observation;
pub mod policy;
pub mod request;
pub mod runtime;
pub mod secrets;
pub mod storage;

#[cfg(all(feature = "worker", target_arch = "wasm32"))]
mod worker_adapter;
