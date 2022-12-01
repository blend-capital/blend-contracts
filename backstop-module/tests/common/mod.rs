use rand::{thread_rng, RngCore};
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env, IntoVal};

// Generics

mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_spec.wasm");
}
pub use token::Client as TokenClient;

mod backstop {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/backstop_module.wasm"
    );
}
pub use backstop::{BackstopError, Client as BackstopClient, Q4W};

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_token_from_id(e: &Env, contract_id: &BytesN<32>, admin: &Identifier) -> TokenClient {
    e.register_contract_token(contract_id);
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
    client
}

pub fn create_backstop_module(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, backstop::WASM);
    (contract_id.clone(), BackstopClient::new(e, contract_id))
}
