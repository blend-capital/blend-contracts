use rand::{thread_rng, RngCore};
use soroban_sdk::{BytesN, Env, IntoVal};
use soroban_auth::Identifier;

// Generics

pub mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_spec.wasm");
}

pub mod lending_pool_wasm {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm");
}

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_token(e: &Env, contract_id: &BytesN<32>, admin: &Identifier) -> token::Client {
    e.register_contract_token(contract_id);
    let client = token::Client::new(e, contract_id);
    let _the_balance = client.balance(admin);
    client.init(
        &admin.clone(), 
        &token::TokenMetadata { 
            name: "unit".into_val(e),
            symbol: "test".into_val(&e),
            decimals: 7 
        }
    );
    client
}

pub fn create_wasm_lending_pool(e: &Env, contract_id: &BytesN<32>) -> lending_pool_wasm::Client {
    e.register_contract_wasm(contract_id, lending_pool_wasm::WASM);
    return lending_pool_wasm::Client::new(e, contract_id);
}

// Contract specific test functions

pub mod pool_helper;