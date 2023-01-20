#![cfg(any(test, feature = "testutils"))]

use crate::constants::POOL_FACTORY;
use rand::{thread_rng, RngCore};
use soroban_sdk::{BytesN, Env};

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
    MockPoolFactoryClient::new(e, contract_id)
}
