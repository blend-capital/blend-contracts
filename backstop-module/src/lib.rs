#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod backstop;
mod dependencies;
mod errors;
mod shares;
mod storage;

pub use crate::backstop::Backstop;
