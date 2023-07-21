use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal};

// Generics

mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_contract.wasm");
}
pub use token::{Client as TokenClient, WASM as TOKEN_WASM};

mod backstop {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/backstop_module.wasm"
    );
}
pub use backstop::{BackstopError, Client as BackstopClient, Q4W};

mod emitter {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/emitter.wasm");
}
pub use emitter::Client as EmitterClient;

mod mock_pool_factory_wasm {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory_wasm::Client as MockPoolFactoryClient;

pub fn create_token<'a>(e: &Env, admin: &Address) -> (Address, TokenClient<'a>) {
    let contract_id = Address::random(&e);
    e.register_contract_wasm(&contract_id, token::WASM);
    let client = TokenClient::new(e, &contract_id);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_id, client)
}

pub fn create_backstop_module(e: &Env) -> (Address, BackstopClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, backstop::WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}

pub fn create_emitter(e: &Env) -> (Address, EmitterClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, emitter::WASM);
    (contract_id.clone(), EmitterClient::new(e, &contract_id))
}

pub fn create_mock_pool_factory(e: &Env) -> (Address, MockPoolFactoryClient) {
    let contract_id = Address::random(&e);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    (
        contract_id.clone(),
        MockPoolFactoryClient::new(e, &contract_id),
    )
}
