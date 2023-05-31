use rand::{thread_rng, RngCore};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

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

pub fn create_wasm_pool_factory(e: &Env) -> (Address, PoolFactoryClient) {
    let contract_address = Address::random(&e);
    e.register_contract_wasm(&contract_address, pool_factory::WASM);
    (
        contract_address.clone(),
        PoolFactoryClient::new(e, &contract_address),
    )
}
