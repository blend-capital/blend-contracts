use soroban_sdk::{testutils::BytesN as _, BytesN, Env};

mod b_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}
pub use b_token::{Asset, Client as DTokenClient, TokenDataKey, TokenError};

pub fn create_b_token(e: &Env) -> (BytesN<32>, DTokenClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, b_token::WASM);
    let client = DTokenClient::new(e, &contract_id);
    (contract_id, client)
}

mod mock_lending_pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_lending_pool.wasm"
    );
}
pub use mock_lending_pool::Client as MockPool;

pub fn create_lending_pool(e: &Env) -> (BytesN<32>, MockPool) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, mock_lending_pool::WASM);
    let client = MockPool::new(e, &contract_id);
    (contract_id, client)
}
