use rand::{thread_rng, RngCore};
use soroban_sdk::{BytesN, Env, IntoVal};

// Generics

mod d_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/d_token.wasm");
}

pub use d_token::{Client as DTokenClient, DTokenError};

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_wasm_d_token(e: &Env) -> (BytesN<32>, DTokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, d_token::WASM);
    (contract_id.clone(), DTokenClient::new(e, contract_id))
}

pub fn create_metadata(e: &Env) -> d_token::TokenMetadata {
    d_token::TokenMetadata {
        name: "unit".into_val(e),
        symbol: "test".into_val(&e),
        decimals: 7,
    }
}
