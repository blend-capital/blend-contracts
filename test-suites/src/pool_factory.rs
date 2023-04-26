use soroban_sdk::{testutils::BytesN as _, BytesN, Env};

mod pool_factory_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/pool_factory.wasm"
    );
}
pub use pool_factory_contract::{
    Client as PoolFactoryClient, PoolInitMeta, WASM as POOL_FACTORY_WASM,
};

pub fn create_pool_factory(e: &Env) -> (BytesN<32>, PoolFactoryClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, pool_factory_contract::WASM);
    (contract_id.clone(), PoolFactoryClient::new(e, &contract_id))
}
