#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod mock_oracle;

pub use mock_oracle::*;
