use rand::{thread_rng, RngCore};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

// Generics

mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_contract.wasm");
}
pub use token::Client as TokenClient;

mod emitter {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/emitter.wasm");
}

pub use emitter::{Client as EmitterClient, EmitterError};

mod backstop {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/backstop_module.wasm"
    );
}
pub use backstop::Client as BackstopClient;

pub fn create_token<'a>(e: &Env, admin: &Address) -> (Address, TokenClient<'a>) {
    let contract_id = e.register_stellar_asset_contract(admin.clone());
    let client = TokenClient::new(e, &contract_id);
    (contract_id, client)
}

pub fn create_wasm_emitter(e: &Env) -> (Address, EmitterClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, emitter::WASM);
    (contract_id.clone(), EmitterClient::new(e, &contract_id))
}

pub fn create_backstop(e: &Env) -> (Address, BackstopClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, backstop::WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}
