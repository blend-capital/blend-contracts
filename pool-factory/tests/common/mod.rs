use rand::{thread_rng, RngCore};
use soroban_sdk::{BytesN, Env};

mod pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/pool_factory.wasm"
    );
}

pub mod lending_pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm"
    );
}

pub mod b_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}

pub mod d_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/d_token.wasm");
}

pub use pool_factory::{Client as PoolFactoryClient, PoolInitMeta};

pub fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

pub fn create_wasm_pool_factory(e: &Env) -> (BytesN<32>, PoolFactoryClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, pool_factory::WASM);
    (contract_id.clone(), PoolFactoryClient::new(e, &contract_id))
}
