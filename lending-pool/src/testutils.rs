#![cfg(any(test, feature = "testutils"))]

use crate::{
    constants::SCALAR_9,
    dependencies::{BackstopClient, TokenClient, BACKSTOP_WASM, TOKEN_WASM},
    pool::Reserve,
    storage::{self, ReserveConfig, ReserveData},
};
use fixed_point_math::FixedPoint;
use soroban_sdk::{testutils::Address as _, unwrap::UnwrapOptimized, Address, Env, IntoVal};

//************************************************
//           External Contract Helpers
//************************************************

// ***** Token *****

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

pub(crate) fn default_reserve(e: &Env) -> Reserve {
    Reserve {
        asset: Address::random(e),
        index: 0,
        l_factor: 0_7500000,
        c_factor: 0_7500000,
        max_util: 0_9500000,
        last_time: 0,
        scalar: 1_0000000,
        d_rate: 1_000_000_000,
        b_rate: 1_000_000_000,
        ir_mod: 1_000_000_000,
        b_supply: 100_0000000,
        d_supply: 75_0000000,
        backstop_credit: 0,
    }
}

pub(crate) fn default_reserve_meta(e: &Env) -> (ReserveConfig, ReserveData) {
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

/// Create a reserve based on the supplied config and data.
///
/// Mints the appropriate amount of underlying tokens to the pool based on the
/// b and d token supply and rates.
///
/// Returns the underlying asset address.
pub(crate) fn create_reserve(
    e: &Env,
    pool_address: &Address,
    token_address: &Address,
    reserve_config: &ReserveConfig,
    reserve_data: &ReserveData,
) {
    let mut new_reserve_config = reserve_config.clone();
    e.as_contract(pool_address, || {
        let index = storage::push_res_list(e, &token_address);
        new_reserve_config.index = index;
        storage::set_res_config(e, &token_address, &new_reserve_config);
        storage::set_res_data(e, &token_address, &reserve_data);
    });
    let underlying_client = TokenClient::new(e, token_address);

    // mint pool assets to set expected b_rate
    let total_supply = reserve_data
        .b_supply
        .fixed_mul_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap_optimized();
    let total_liabilities = reserve_data
        .d_supply
        .fixed_mul_floor(reserve_data.d_rate, SCALAR_9)
        .unwrap_optimized();
    let to_mint_pool = total_supply - total_liabilities - reserve_data.backstop_credit;
    underlying_client
        .mock_all_auths()
        .mint(&pool_address, &to_mint_pool);
}
