#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod backstop_manager;
mod constants;
mod contract;
mod emitter;
mod errors;
mod storage;
mod testutils;

pub use backstop_manager::Swap;
pub use contract::*;
pub use errors::EmitterError;
pub use storage::EmitterDataKey;
