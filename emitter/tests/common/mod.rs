use rand::{thread_rng, RngCore};
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env, IntoVal};

// Generics

mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_contract.wasm");
}
pub use token::Client as TokenClient;

mod emitter {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/emitter.wasm");
}

pub use emitter::{Client as EmitterClient, EmitterError};

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_token(e: &Env, admin: &Identifier) -> (BytesN<32>, TokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, token::WASM);
    let client = TokenClient::new(e, contract_id.clone());
    client.initialize(
        &admin,
        &7,
        &"unit".into_val(e),
        &"test".into_val(&e),
    );
    (contract_id, client)
}

pub fn create_wasm_emitter(e: &Env) -> (BytesN<32>, EmitterClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, emitter::WASM);
    (contract_id.clone(), EmitterClient::new(e, contract_id))
}
