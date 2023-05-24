use soroban_sdk::{testutils::Address as _, Address, Env};

mod d_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/d_token.wasm");
}
pub use d_token::{Asset, Client as DTokenClient, TokenError};

pub fn create_d_token(e: &Env) -> (Address, DTokenClient) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, d_token::WASM);
    let client = DTokenClient::new(e, &contract_address);
    (contract_address, client)
}
