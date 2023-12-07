use soroban_sdk::{testutils::Address as _, Address, Env};

mod pool_factory_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/pool_factory.wasm"
    );
}
use pool_factory::PoolFactoryClient;

use mock_pool_factory::MockPoolFactory;

pub fn create_pool_factory<'a>(e: &Env, wasm: bool) -> (Address, PoolFactoryClient<'a>) {
    let contract_id = Address::generate(e);
    if wasm {
        e.register_contract_wasm(&contract_id, pool_factory_contract::WASM);
    } else {
        e.register_contract(&contract_id, MockPoolFactory {});
    }
    (contract_id.clone(), PoolFactoryClient::new(e, &contract_id))
}
