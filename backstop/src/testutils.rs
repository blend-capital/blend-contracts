#![cfg(test)]

use crate::{
    backstop::Q4W,
    dependencies::{CometClient, TokenClient, COMET_WASM, TOKEN_WASM},
    storage::{self},
    BackstopContract,
};

use soroban_sdk::{
    testutils::Address as _, unwrap::UnwrapOptimized, vec, Address, Env, IntoVal, Vec,
};

use mock_pool_factory::{MockPoolFactory, MockPoolFactoryClient};

pub(crate) fn create_backstop(e: &Env) -> Address {
    e.register_contract(None, BackstopContract {})
}

pub(crate) fn create_token<'a>(e: &Env, admin: &Address) -> (Address, TokenClient<'a>) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, TOKEN_WASM);
    let client = TokenClient::new(e, &contract_address);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_address, client)
}

pub(crate) fn create_blnd_token<'a>(
    e: &Env,
    backstop: &Address,
    admin: &Address,
) -> (Address, TokenClient<'a>) {
    let (contract_address, client) = create_token(e, admin);

    e.as_contract(backstop, || {
        storage::set_blnd_token(e, &contract_address);
    });
    (contract_address, client)
}

pub(crate) fn create_usdc_token<'a>(
    e: &Env,
    backstop: &Address,
    admin: &Address,
) -> (Address, TokenClient<'a>) {
    let (contract_address, client) = create_token(e, admin);

    e.as_contract(backstop, || {
        storage::set_usdc_token(e, &contract_address);
    });
    (contract_address, client)
}

pub(crate) fn create_backstop_token<'a>(
    e: &Env,
    backstop: &Address,
    admin: &Address,
) -> (Address, TokenClient<'a>) {
    let (contract_address, client) = create_token(e, admin);

    e.as_contract(backstop, || {
        storage::set_backstop_token(e, &contract_address);
    });
    (contract_address, client)
}

pub(crate) fn create_mock_pool_factory<'a>(
    e: &Env,
    backstop: &Address,
) -> (Address, MockPoolFactoryClient<'a>) {
    let contract_address = e.register_contract(None, MockPoolFactory {});
    e.as_contract(backstop, || {
        storage::set_pool_factory(e, &contract_address);
    });
    (
        contract_address.clone(),
        MockPoolFactoryClient::new(e, &contract_address),
    )
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
) -> (Address, CometClient<'a>) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, COMET_WASM);
    let client = CometClient::new(e, &contract_address);

    let blnd_client = TokenClient::new(e, blnd_token);
    let usdc_client = TokenClient::new(e, usdc_token);
    blnd_client.mint(&admin, &1_000_0000000);
    usdc_client.mint(&admin, &25_0000000);
    let exp_ledger = e.ledger().sequence() + 100;
    blnd_client.approve(&admin, &contract_address, &2_000_0000000, &exp_ledger);
    usdc_client.approve(&admin, &contract_address, &2_000_0000000, &exp_ledger);

    client.init(&Address::random(e), &admin);
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

/********** Comparison Helpers **********/

pub(crate) fn assert_eq_vec_q4w(actual: &Vec<Q4W>, expected: &Vec<Q4W>) {
    assert_eq!(actual.len(), expected.len());
    for index in 0..actual.len() {
        let actual_q4w = actual.get(index).unwrap_optimized();
        let expected_q4w = expected.get(index).unwrap_optimized();
        assert_eq!(actual_q4w.amount, expected_q4w.amount);
        assert_eq!(actual_q4w.exp, expected_q4w.exp);
    }
}
