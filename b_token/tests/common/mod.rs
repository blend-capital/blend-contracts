use rand::{thread_rng, RngCore};
use soroban_sdk::{BytesN, Env, IntoVal};

// Generics

mod b_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}

pub use b_token::{Client as BTokenClient, TokenError};

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_wasm_b_token(e: &Env) -> (BytesN<32>, BTokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, b_token::WASM);
    (contract_id.clone(), BTokenClient::new(e, contract_id))
}

pub fn create_metadata(e: &Env) -> b_token::TokenMetadata {
    b_token::TokenMetadata {
        name: "unit".into_val(e),
        symbol: "test".into_val(&e),
        decimals: 7,
    }
}
