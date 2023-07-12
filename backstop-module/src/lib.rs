#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod backstop;
mod constants;
mod contract;
mod dependencies;
mod emissions;
mod errors;
mod storage;
mod testutils;

pub use crate::contract::BackstopModuleContract;
