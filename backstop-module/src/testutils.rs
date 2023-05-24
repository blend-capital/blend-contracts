#![cfg(any(test, feature = "testutils"))]

use crate::{
    dependencies::{TokenClient, TOKEN_WASM},
    storage::{self, Q4W},
};
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, Vec};

mod mock_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory::Client as MockPoolFactoryClient;

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
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, mock_pool_factory::WASM);

    e.as_contract(backstop, || {
        storage::set_pool_factory(e, &contract_address);
    });
    (
        contract_address.clone(),
        MockPoolFactoryClient::new(e, &contract_address),
    )
}

/********** Comparison Helpers **********/

pub(crate) fn assert_eq_vec_q4w(actual: &Vec<Q4W>, expected: &Vec<Q4W>) {
    assert_eq!(actual.len(), expected.len());
    for index in 0..actual.len() {
        let actual_q4w = actual.get(index).unwrap().unwrap();
        let expected_q4w = expected.get(index).unwrap().unwrap();
        assert_eq!(actual_q4w.amount, expected_q4w.amount);
        assert_eq!(actual_q4w.exp, expected_q4w.exp);
    }
}
