mod pool;
pub use pool::Client as PoolClient;
#[cfg(any(test, feature = "testutils"))]
pub use pool::WASM as POOL_WASM;
