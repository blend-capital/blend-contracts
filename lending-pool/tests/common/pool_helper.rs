use crate::common::{create_token, B_TOKEN_WASM, D_TOKEN_WASM};
use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    Address, BytesN, Env,
};

use super::{PoolClient, ReserveConfig, ReserveMetadata};

/// Set up pool
pub fn setup_pool(
    e: &Env,
    pool_client: &PoolClient,
    admin: &Address,
    oracle_id: &BytesN<32>,
    backstop_id: &BytesN<32>,
    bstop_rate: u64,
) {
    let b_token_hash = e.install_contract_wasm(B_TOKEN_WASM);
    let d_token_hash = e.install_contract_wasm(D_TOKEN_WASM);
    let backstop = Address::from_contract_id(e, backstop_id);
    pool_client.initialize(
        admin,
        oracle_id,
        backstop_id,
        &backstop,
        &bstop_rate,
        &b_token_hash,
        &d_token_hash,
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
    e: &Env,
    pool: &Address,
    pool_client: &PoolClient,
    admin: &Address,
    metadata: &ReserveMetadata,
) -> (BytesN<32>, BytesN<32>, BytesN<32>) {
    let (asset_id, _) = create_token(e, admin);

    pool_client.init_res(&admin, &asset_id, &metadata);
    let reserve_config = pool_client.res_config(&asset_id);

    return (asset_id, reserve_config.b_token, reserve_config.d_token);
}
