#![cfg(any(test, feature = "testutils"))]

use crate::{
    dependencies::{
        BackstopClient, BlendTokenClient, TokenClient, BACKSTOP_WASM, B_TOKEN_WASM, D_TOKEN_WASM,
        TOKEN_WASM,
    },
    reserve::Reserve,
    storage::{self, ReserveConfig, ReserveData},
};
use rand::{thread_rng, RngCore};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, IntoVal};

//************************************************
//           External Contract Helpers
//************************************************

//***** Token *****

pub(crate) fn create_token_contract<'a>(e: &Env, admin: &Address) -> (Address, TokenClient<'a>) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, TOKEN_WASM);
    let client = TokenClient::new(e, &contract_address);
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_address, client)
}

pub(crate) fn create_token_from_id<'a>(
    e: &Env,
    contract_address: &Address,
    admin: &Address,
) -> TokenClient<'a> {
    e.register_contract_wasm(contract_address, TOKEN_WASM);
    let client = TokenClient::new(e, contract_address);
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(e));
    client
}

pub(crate) fn create_blnd_token<'a>(
    e: &Env,
    pool_address: &Address,
    admin: &Address,
) -> (Address, TokenClient<'a>) {
    let (contract_address, client) = create_token_contract(e, admin);

    e.as_contract(pool_address, || {
        storage::set_blnd_token(e, &contract_address);
    });
    (contract_address, client)
}

pub(crate) fn create_usdc_token<'a>(
    e: &Env,
    pool_address: &Address,
    admin: &Address,
) -> (Address, TokenClient<'a>) {
    let (contract_address, client) = create_token_contract(e, admin);

    e.as_contract(pool_address, || {
        storage::set_usdc_token(e, &contract_address);
    });
    (contract_address, client)
}

pub(crate) fn create_b_token_from_id<'a>(
    e: &Env,
    contract_address: &Address,
    pool_address: &Address,
    asset: &Address,
    res_index: u32,
) -> BlendTokenClient<'a> {
    e.register_contract_wasm(contract_address, B_TOKEN_WASM);
    let client = BlendTokenClient::new(e, contract_address);
    client.initialize(pool_address, &7, &"unit".into_val(e), &"test".into_val(e));
    client.initialize_asset(pool_address, asset, &res_index);
    client
}

pub(crate) fn create_d_token_from_id<'a>(
    e: &Env,
    contract_address: &Address,
    pool_address: &Address,
    asset: &Address,
    res_index: u32,
) -> BlendTokenClient<'a> {
    e.register_contract_wasm(contract_address, D_TOKEN_WASM);
    let client = BlendTokenClient::new(e, contract_address);
    client.initialize(pool_address, &7, &"unit".into_val(e), &"test".into_val(e));
    client.initialize_asset(pool_address, asset, &res_index);
    client
}

//***** Oracle ******
// TODO: Avoid WASM-ing unit tests by adding conditional `rlib` for test builds
//       -> https://rust-lang.github.io/rfcs/3180-cargo-cli-crate-type.html
// use mock_blend_oracle::testutils::register_test_mock_oracle;

mod mock_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}
pub(crate) use mock_oracle::Client as MockOracleClient;

pub(crate) fn create_mock_oracle(e: &Env) -> (Address, MockOracleClient) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, mock_oracle::WASM);
    (
        contract_address.clone(),
        MockOracleClient::new(e, &contract_address),
    )
}

//***** Pool Factory ******

mod mock_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory::Client as MockPoolFactoryClient;

pub(crate) fn create_mock_pool_factory(e: &Env) -> (Address, MockPoolFactoryClient) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, mock_pool_factory::WASM);
    (
        contract_address.clone(),
        MockPoolFactoryClient::new(e, &contract_address),
    )
}

//***** Backstop ******

pub(crate) fn create_backstop(e: &Env) -> (Address, BackstopClient) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, BACKSTOP_WASM);
    (
        contract_address.clone(),
        BackstopClient::new(e, &contract_address),
    )
}

pub(crate) fn setup_backstop(
    e: &Env,
    pool_address: &Address,
    backstop_id: &Address,
    backstop_token: &Address,
    blnd_token: &Address,
) {
    let (pool_factory, mock_pool_factory_client) = create_mock_pool_factory(e);
    mock_pool_factory_client.set_pool(pool_address);
    BackstopClient::new(e, backstop_id).initialize(backstop_token, blnd_token, &pool_factory);
    e.as_contract(pool_address, || {
        storage::set_backstop(e, backstop_id);
    });
}

//************************************************
//            Object Creation Helpers
//************************************************

//***** Reserve *****

pub(crate) fn create_reserve(e: &Env) -> Reserve {
    Reserve {
        asset: Address::random(&e),
        config: ReserveConfig {
            b_token: Address::random(&e),
            d_token: Address::random(&e),
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
        data: ReserveData {
            d_rate: 1_000_000_000,
            ir_mod: 1_000_000_000,
            b_supply: 100_0000000,
            d_supply: 75_0000000,
            last_time: 0,
        },
        b_rate: Some(1_000_000_000),
    }
}

/// Expects a valid b_rate to be set
pub(crate) fn setup_reserve(
    e: &Env,
    pool_address: &Address,
    admin: &Address,
    reserve: &mut Reserve,
) {
    e.as_contract(pool_address, || {
        let index = storage::push_res_list(e, &reserve.asset);
        reserve.config.index = index;
        storage::set_res_config(e, &reserve.asset, &reserve.config);
        storage::set_res_data(e, &reserve.asset, &reserve.data);
    });
    let asset_client = create_token_from_id(e, &reserve.asset, admin);
    create_b_token_from_id(
        e,
        &reserve.config.b_token,
        pool_address,
        &reserve.asset,
        reserve.config.index,
    );
    create_d_token_from_id(
        e,
        &reserve.config.d_token,
        pool_address,
        &reserve.asset,
        reserve.config.index,
    );

    // mint pool assets to set expected b_rate
    let to_mint_pool = reserve.total_supply(e) - reserve.total_liabilities();
    asset_client.mint(&pool_address, &to_mint_pool);
}
