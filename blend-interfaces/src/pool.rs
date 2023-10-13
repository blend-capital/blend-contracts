pub use pool::{
    Pool, PoolClient, PoolConfig, PoolDataKey, PoolError, Positions, Request, ReserveConfig,
    ReserveData, ReserveEmissionMetadata, ReserveEmissionsConfig, ReserveEmissionsData,
};

mod wasm {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/pool.wasm");
}
pub use wasm::WASM as PoolWASM;
