use rand::{thread_rng, RngCore};
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env, IntoVal};

// Generics

mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_spec.wasm");
}
pub use token::Client as TokenClient;

mod pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm"
    );
}
pub use pool::{Client as PoolClient, PoolError, ReserveConfig, ReserveData};

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

pub fn create_token(e: &Env, admin: &Identifier) -> (BytesN<32>, TokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_token(&contract_id);
    let client = TokenClient::new(e, contract_id.clone());
    let _the_balance = client.balance(admin);
    client.init(
        &admin.clone(),
        &token::TokenMetadata {
            name: "unit".into_val(e),
            symbol: "test".into_val(&e),
            decimals: 7,
        },
    );
    (contract_id, client)
}

pub fn create_wasm_lending_pool(e: &Env) -> (BytesN<32>, PoolClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, pool::WASM);
    (contract_id.clone(), PoolClient::new(e, contract_id))
}

pub fn create_mock_oracle(e: &Env) -> (BytesN<32>, MockOracleClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, mock_blend_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, contract_id))
}

// Contract specific test functions

pub mod pool_helper;
