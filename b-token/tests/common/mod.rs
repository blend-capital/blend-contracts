use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

mod b_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}
pub use b_token::{Asset, Client as BTokenClient, TokenDataKey, TokenError};

pub fn create_b_token<'a>(e: &Env) -> (Address, BTokenClient<'a>) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, b_token::WASM);
    let client = BTokenClient::new(e, &contract_address);
    (contract_address, client)
}

mod mock_lending_pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_lending_pool.wasm"
    );
}
pub use mock_lending_pool::Client as MockPool;

pub fn create_lending_pool<'a>(e: &Env) -> (Address, MockPool<'a>) {
    let contract_address = Address::random(e);
    e.register_contract_wasm(&contract_address, mock_lending_pool::WASM);
    let client = MockPool::new(e, &contract_address);
    (contract_address, client)
}
