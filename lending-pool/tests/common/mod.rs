use rand::{thread_rng, RngCore};
use soroban_sdk::{Address, BytesN, Env, IntoVal};

// Generics

mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_contract.wasm");
}
pub use token::Client as TokenClient;

mod b_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}
pub use b_token::{Client as BlendTokenClient, WASM as B_TOKEN_WASM};

mod d_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}
pub use d_token::WASM as D_TOKEN_WASM;

mod pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm"
    );
}
pub use pool::{
    AuctionData, Client as PoolClient, LiquidationMetadata, PoolError, ReserveConfig, ReserveData,
    ReserveMetadata,
};

mod mock_blend_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}
pub use mock_blend_oracle::{Client as MockOracleClient, OracleError};

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_stellar_token(e: &Env, admin: &Address) -> (BytesN<32>, TokenClient) {
    let contract_id = e.register_stellar_asset_contract(admin.clone());
    let client = TokenClient::new(e, &contract_id);
    (contract_id, client)
}

pub fn create_token(e: &Env, admin: &Address) -> (BytesN<32>, TokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, token::WASM);
    let client = TokenClient::new(e, &contract_id);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_id, client)
}

pub fn create_token_from_id(e: &Env, contract_id: &BytesN<32>, admin: &Address) -> TokenClient {
    e.register_contract_wasm(contract_id, token::WASM);
    let client = TokenClient::new(e, contract_id);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));
    client
}

pub fn create_wasm_lending_pool(e: &Env) -> (BytesN<32>, PoolClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, pool::WASM);
    (contract_id.clone(), PoolClient::new(e, &contract_id))
}

pub fn create_mock_oracle(e: &Env) -> (BytesN<32>, MockOracleClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, mock_blend_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, &contract_id))
}

// Contract specific test functions

pub mod pool_helper;
