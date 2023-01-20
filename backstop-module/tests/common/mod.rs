use rand::{thread_rng, RngCore};
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env, IntoVal};

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

pub fn create_token_from_id(e: &Env, contract_id: &BytesN<32>, admin: &Identifier) -> TokenClient {
    e.register_contract_wasm(contract_id, token::WASM);
    let client = TokenClient::new(e, contract_id.clone());
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(&e));
    client
}

pub fn create_backstop_module(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, backstop::WASM);
    (contract_id.clone(), BackstopClient::new(e, contract_id))
}

pub fn create_mock_pool_factory(e: &Env) -> MockPoolFactoryClient {
    let contract_id = BytesN::from_array(&e, &[101; 32]);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    MockPoolFactoryClient::new(e, contract_id)
}
