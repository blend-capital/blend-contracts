use crate::common::{create_token, B_TOKEN_WASM, D_TOKEN_WASM};
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

use super::{
    create_backstop, create_mock_pool_factory, create_token_from_id, PoolClient, ReserveMetadata,
};

/// Set up pool
pub fn setup_pool(
    e: &Env,
    pool_id: &Address,
    pool_client: &PoolClient,
    admin: &Address,
    name: &Symbol,
    oracle_id: &Address,
    bstop_rate: u64,
    blnd_id: &Address,
    usdc_id: &Address,
) -> Address {
    let b_token_hash = e.install_contract_wasm(B_TOKEN_WASM);
    let d_token_hash = e.install_contract_wasm(D_TOKEN_WASM);

    let (pool_factory_id, pool_factory_client) = create_mock_pool_factory(&e);
    pool_factory_client.set_pool(&pool_id);

    let backstop_id = create_and_setup_backstop(e, pool_id, admin, blnd_id, &pool_factory_id);
    pool_client.initialize(
        admin,
        name,
        oracle_id,
        &bstop_rate,
        &backstop_id,
        &b_token_hash,
        &d_token_hash,
        &blnd_id,
        &usdc_id,
    );
    pool_client.set_status(admin, &0);
    backstop_id
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
    pool_client: &PoolClient,
    admin: &Address,
    metadata: &ReserveMetadata,
) -> (Address, Address, Address) {
    let (asset_id, _) = create_token(e, admin);

    pool_client.init_res(&admin, &asset_id, &metadata);
    let reserve_config = pool_client.res_config(&asset_id);

    return (asset_id, reserve_config.b_token, reserve_config.d_token);
}

/// Set up backstop
pub fn create_and_setup_backstop(
    e: &Env,
    pool_id: &Address,
    admin: &Address,
    blnd_id: &Address,
    pool_factory_id: &Address,
) -> Address {
    let (backstop_id, backstop_client) = create_backstop(e);
    let backstop_token_id = Address::random(&e);
    let backstop_token_client = create_token_from_id(e, &backstop_token_id, admin);
    backstop_client.initialize(&backstop_token_id, blnd_id, &pool_factory_id);

    // deposit minimum deposit amount into backstop for pool
    backstop_token_client.mint(admin, &1_100_000_0000000);
    backstop_client.deposit(admin, &pool_id, &1_100_000_0000000);

    backstop_id
}
