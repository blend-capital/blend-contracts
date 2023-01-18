#![cfg(any(test, feature = "testutils"))]

use crate::{
    constants::POOL_FACTORY,
    dependencies::{BackstopClient, TokenClient, BACKSTOP_WASM, TOKEN_WASM},
};
use rand::{thread_rng, RngCore};
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env, IntoVal};
// TODO: Avoid WASM-ing unit tests by adding conditional `rlib` for test builds
//       -> https://rust-lang.github.io/rfcs/3180-cargo-cli-crate-type.html
// use mock_blend_oracle::testutils::register_test_mock_oracle;

mod mock_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}
pub(crate) use mock_oracle::Client as MockOracleClient;

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

pub(crate) fn create_token_contract(e: &Env, admin: &Identifier) -> (BytesN<32>, TokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, contract_id.clone());
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(&e));
    (contract_id, client)
}

pub(crate) fn create_mock_oracle(e: &Env) -> (BytesN<32>, MockOracleClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, mock_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, contract_id))
}

pub(crate) fn create_backstop(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, BACKSTOP_WASM);
    (contract_id.clone(), BackstopClient::new(e, contract_id))
}

pub fn create_token_from_id(e: &Env, contract_id: &BytesN<32>, admin: &Identifier) -> TokenClient {
    e.register_contract_wasm(contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, contract_id.clone());
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(&e));
    client
}

pub(crate) fn create_mock_pool_factory(e: &Env) -> MockPoolFactoryClient {
    let contract_id = BytesN::from_array(&e, &POOL_FACTORY);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    MockPoolFactoryClient::new(e, contract_id)
}
