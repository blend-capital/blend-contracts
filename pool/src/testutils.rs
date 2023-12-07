#![cfg(test)]

use crate::{
    constants::{SCALAR_7, SCALAR_9},
    pool::Reserve,
    storage::{self, ReserveConfig, ReserveData},
    PoolContract,
};
use emitter::{EmitterClient, EmitterContract};
use sep_40_oracle::testutils::{MockPriceOracleClient, MockPriceOracleWASM};
use sep_41_token::testutils::{MockTokenClient, MockTokenWASM};
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{
    map, testutils::Address as _, unwrap::UnwrapOptimized, vec, Address, Env, IntoVal,
};

use backstop::{BackstopClient, BackstopContract};
use mock_pool_factory::{MockPoolFactory, MockPoolFactoryClient};

pub(crate) fn create_pool(e: &Env) -> Address {
    e.register_contract(None, PoolContract {})
}

//************************************************
//           External Contract Helpers
//************************************************

// ***** Token *****

pub(crate) fn create_token_contract<'a>(
    e: &Env,
    admin: &Address,
) -> (Address, MockTokenClient<'a>) {
    let contract_address = Address::generate(e);
    e.register_contract_wasm(&contract_address, MockTokenWASM);
    let client = MockTokenClient::new(e, &contract_address);
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_address, client)
}

pub(crate) fn create_blnd_token<'a>(
    e: &Env,
    pool_address: &Address,
    admin: &Address,
) -> (Address, MockTokenClient<'a>) {
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
) -> (Address, MockTokenClient<'a>) {
    let (contract_address, client) = create_token_contract(e, admin);

    e.as_contract(pool_address, || {
        storage::set_usdc_token(e, &contract_address);
    });
    (contract_address, client)
}

//***** Oracle ******

pub(crate) fn create_mock_oracle(e: &Env) -> (Address, MockPriceOracleClient) {
    let contract_address = e.register_contract_wasm(None, MockPriceOracleWASM);
    (
        contract_address.clone(),
        MockPriceOracleClient::new(e, &contract_address),
    )
}

//***** Pool Factory ******

pub(crate) fn create_mock_pool_factory(e: &Env) -> (Address, MockPoolFactoryClient) {
    let contract_address = e.register_contract(None, MockPoolFactory {});
    (
        contract_address.clone(),
        MockPoolFactoryClient::new(e, &contract_address),
    )
}

//***** Pool Factory ******

pub(crate) fn create_emitter<'a>(
    e: &Env,
    backstop_id: &Address,
    backstop_token: &Address,
    blnd_token: &Address,
) -> (Address, EmitterClient<'a>) {
    let contract_address = e.register_contract(None, EmitterContract {});
    let client = EmitterClient::new(e, &contract_address);
    client.initialize(blnd_token, backstop_id, backstop_token);
    (contract_address.clone(), client)
}

//***** Backstop ******

mod comet {
    soroban_sdk::contractimport!(file = "../comet.wasm");
}

pub(crate) fn create_backstop(e: &Env) -> (Address, BackstopClient) {
    let contract_address = e.register_contract(None, BackstopContract {});
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
    usdc_token: &Address,
    blnd_token: &Address,
) {
    let (pool_factory, mock_pool_factory_client) = create_mock_pool_factory(e);
    mock_pool_factory_client.set_pool(pool_address);
    let (emitter, _) = create_emitter(e, backstop_id, backstop_token, blnd_token);
    BackstopClient::new(e, backstop_id).initialize(
        backstop_token,
        &emitter,
        usdc_token,
        blnd_token,
        &pool_factory,
        &map![e, (pool_address.clone(), 50_000_000 * SCALAR_7)],
    );
    e.as_contract(pool_address, || {
        storage::set_backstop(e, backstop_id);
    });
}

/// Deploy a test Comet LP pool of 80% BLND / 20% USDC and set it as the backstop token.
///
/// Initializes the pool with the following settings:
/// - Swap fee: 0.3%
/// - BLND: 1,000
/// - USDC: 25
/// - Shares: 100
pub(crate) fn create_comet_lp_pool<'a>(
    e: &Env,
    admin: &Address,
    blnd_token: &Address,
    usdc_token: &Address,
) -> (Address, comet::Client<'a>) {
    let contract_address = Address::generate(e);
    e.register_contract_wasm(&contract_address, comet::WASM);
    let client = comet::Client::new(e, &contract_address);

    let blnd_client = MockTokenClient::new(e, blnd_token);
    let usdc_client = MockTokenClient::new(e, usdc_token);
    blnd_client.mint(&admin, &1_000_0000000);
    usdc_client.mint(&admin, &25_0000000);
    let exp_ledger = e.ledger().sequence() + 100;
    blnd_client.approve(&admin, &contract_address, &2_000_0000000, &exp_ledger);
    usdc_client.approve(&admin, &contract_address, &2_000_0000000, &exp_ledger);

    client.init(&Address::generate(e), &admin);
    client.bundle_bind(
        &vec![e, blnd_token.clone(), usdc_token.clone()],
        &vec![e, 1_000_0000000, 25_0000000],
        &vec![e, 8_0000000, 2_0000000],
    );

    client.set_swap_fee(&0_0030000, &admin);
    client.finalize();
    client.set_public_swap(&admin, &true);

    (contract_address, client)
}

//************************************************
//            Object Creation Helpers
//************************************************

//***** Reserve *****

pub(crate) fn default_reserve(e: &Env) -> Reserve {
    Reserve {
        asset: Address::generate(e),
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

pub(crate) fn default_reserve_meta() -> (ReserveConfig, ReserveData) {
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
    let underlying_client = MockTokenClient::new(e, token_address);

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
