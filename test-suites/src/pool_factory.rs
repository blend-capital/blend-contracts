use soroban_sdk::{testutils::Address as _, Address, Env};

mod pool_factory_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/pool_factory.wasm"
    );
}
pub use pool_factory_contract::{
    Client as PoolFactoryClient, PoolInitMeta, WASM as POOL_FACTORY_WASM,
};

pub fn create_pool_factory<'a>(e: &Env) -> (Address, PoolFactoryClient<'a>) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, pool_factory_contract::WASM);
    (contract_id.clone(), PoolFactoryClient::new(e, &contract_id))
}
