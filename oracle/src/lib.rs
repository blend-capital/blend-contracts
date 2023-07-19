#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod blend_oracle;

pub use blend_oracle::*;
