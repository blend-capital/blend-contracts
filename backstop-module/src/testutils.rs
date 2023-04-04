#![cfg(any(test, feature = "testutils"))]

use crate::{
    constants::POOL_FACTORY,
    dependencies::{TokenClient, TOKEN_WASM},
    storage::{self, Q4W},
};
use rand::{thread_rng, RngCore};
use soroban_sdk::{testutils::BytesN as _, Address, BytesN, Env, IntoVal, Vec};

mod mock_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory::Client as MockPoolFactoryClient;

pub(crate) fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub(crate) fn create_mock_pool_factory(e: &Env) -> MockPoolFactoryClient {
    let contract_id = BytesN::from_array(&e, &POOL_FACTORY);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    MockPoolFactoryClient::new(e, &contract_id)
}

pub(crate) fn create_backstop_token(
    e: &Env,
    backstop: &BytesN<32>,
    admin: &Address,
) -> (BytesN<32>, TokenClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, &contract_id);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));

    e.as_contract(backstop, || {
        storage::set_backstop_token(e, &contract_id);
    });
    (contract_id, client)
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
