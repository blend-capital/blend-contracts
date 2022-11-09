#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod mock_blend_oracle;

pub use crate::mock_blend_oracle::{MockBlendOracle, MockBlendOracleClient};
