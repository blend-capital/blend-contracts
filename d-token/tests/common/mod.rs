use soroban_sdk::{Env, BytesN, testutils::{BytesN as _}};

mod d_token {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/d_token.wasm"
    );
}
pub use d_token::{
    Asset, Client as DTokenClient, TokenError
};

pub fn create_d_token(e: &Env) -> (BytesN<32>, DTokenClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, d_token::WASM);
    let client = DTokenClient::new(e, &contract_id);
    (contract_id, client)
}