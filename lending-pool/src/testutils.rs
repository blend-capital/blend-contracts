#![cfg(any(test, feature = "testutils"))]

use crate::dependencies::{TokenClient, TokenMetadata};
use rand::{thread_rng, RngCore};
use soroban_auth::Identifier;
use soroban_sdk::{AccountId, BytesN, Env, IntoVal};
// TODO: Avoid WASM-ing unit tests by adding conditional `rlib` for test builds
//       -> https://rust-lang.github.io/rfcs/3180-cargo-cli-crate-type.html
// use mock_blend_oracle::testutils::register_test_mock_oracle;

mod mock_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}

pub(crate) use mock_oracle::Client as MockOracleClient;

pub(crate) fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub(crate) fn create_token_contract(e: &Env, admin: &AccountId) -> (BytesN<32>, TokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_token(&contract_id);

    let token = TokenClient::new(e, contract_id.clone());
    token.init(
        &Identifier::Account(admin.clone()),
        &TokenMetadata {
            name: "unit".into_val(e),
            symbol: "test".into_val(e),
            decimals: 7,
        },
    );
    (contract_id, token)
}

pub(crate) fn create_mock_oracle(e: &Env) -> (BytesN<32>, MockOracleClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, mock_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, contract_id))
}
