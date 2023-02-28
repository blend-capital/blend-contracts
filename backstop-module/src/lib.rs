#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod backstop;
mod constants;
mod contract;
mod dependencies;
mod distributor;
mod errors;
mod pool;
mod storage;
mod testutils;
mod user;

pub use crate::contract::BackstopModuleContract;
