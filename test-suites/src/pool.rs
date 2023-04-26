mod pool_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/lending_pool.wasm"
    );
}
pub use pool_contract::{
    AuctionData, Client as PoolClient, LiquidationMetadata, PoolError, ReserveConfig, ReserveData,
    ReserveEmissionMetadata, ReserveMetadata, WASM as POOL_WASM,
};

pub fn default_reserve_metadata() -> ReserveMetadata {
    ReserveMetadata {
        decimals: 7,
        c_factor: 0_7500000,
        l_factor: 0_7500000,
        util: 0_5000000,
        max_util: 0_9500000,
        r_one: 0_0500000,
        r_two: 0_5000000,
        r_three: 1_5000000,
        reactivity: 100, // 10e-5
    }
}
