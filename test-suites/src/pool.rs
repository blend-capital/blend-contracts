mod pool_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/lending_pool.wasm"
    );
}
use crate::b_token;
use crate::d_token;
pub use pool_contract::{
    AuctionData, Client as PoolClient, LiquidationMetadata, PoolError, ReserveConfig, ReserveData,
    ReserveMetadata, WASM as POOL_WASM,
};
use soroban_sdk::{Address, BytesN, Env};

pub fn setup_pool(
    e: &Env,
    pool_id: &BytesN<32>,
    pool_client: &PoolClient,
    admin: &Address,
    oracle_id: &BytesN<32>,
    bstop_rate: u64,
    blnd_id: &BytesN<32>,
    usdc_id: &BytesN<32>,
    backstop_id: &BytesN<32>,
) {
    let b_token_hash = e.install_contract_wasm(b_token::B_TOKEN_WASM);
    let d_token_hash = e.install_contract_wasm(d_token::D_TOKEN_WASM);

    pool_client.initialize(
        admin,
        oracle_id,
        &bstop_rate,
        backstop_id,
        &b_token_hash,
        &d_token_hash,
        &blnd_id,
        &usdc_id,
    );
    pool_client.set_status(admin, &0);
}

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

/// Uses default configuration
pub fn setup_reserve(
    pool_client: &PoolClient,
    admin: &Address,
    metadata: &ReserveMetadata,
    asset_id: &BytesN<32>,
) -> (BytesN<32>, BytesN<32>, BytesN<32>) {
    pool_client.init_res(&admin, &asset_id, &metadata);
    let reserve_config = pool_client.res_config(&asset_id);

    return (
        asset_id.clone(),
        reserve_config.b_token,
        reserve_config.d_token,
    );
}
