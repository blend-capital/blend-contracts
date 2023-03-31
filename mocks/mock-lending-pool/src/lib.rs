#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod lending_pool;

mod storage;

pub use crate::lending_pool::MockLendingPool;
