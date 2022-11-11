#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod pool;
mod storage;
mod types;

mod dependencies;
pub mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_spec.wasm");
}

pub use crate::pool::{PoolClient, Pool};
