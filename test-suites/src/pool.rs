mod pool_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm"
    );
}
pub use pool_contract::{
    AuctionData, Client as PoolClient, LiquidationMetadata, PoolDataKey, PoolError, Request,
    ReserveConfig, ReserveData, ReserveEmissionMetadata, ReserveEmissionsConfig, UserEmissionData,
    UserReserveKey, WASM as POOL_WASM,
};

pub fn default_reserve_metadata() -> (ReserveConfig, ReserveData) {
    (
        ReserveConfig {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_002_000, // 10e-5
            index: 0,
        },
        ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 1_000_000_000,
            b_supply: 100_0000000,
            d_supply: 75_0000000,
            last_time: 0,
            backstop_credit: 0,
        },
    )
}
