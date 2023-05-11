use rand::{thread_rng, RngCore};
use soroban_sdk::{testutils::BytesN as _, Address, BytesN, Env, IntoVal};

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

mod mock_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory::Client as MockPoolFactoryClient;

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_token(e: &Env, admin: &Address) -> (BytesN<32>, TokenClient) {
    let contract_id = BytesN::<32>::random(&e);
    e.register_contract_wasm(&contract_id, token::WASM);
    let client = TokenClient::new(e, &contract_id);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_id, client)
}

pub fn create_backstop_module(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, backstop::WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}

pub fn create_emitter(e: &Env) -> (BytesN<32>, EmitterClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, emitter::WASM);
    (contract_id.clone(), EmitterClient::new(e, &contract_id))
}

pub fn create_mock_pool_factory(e: &Env) -> (BytesN<32>, MockPoolFactoryClient) {
    let contract_id = BytesN::<32>::random(&e);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    (
        contract_id.clone(),
        MockPoolFactoryClient::new(e, &contract_id),
    )
}
