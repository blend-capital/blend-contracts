use soroban_sdk::{testutils::Address as _, Address, Env};

mod backstop_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/backstop_module.wasm"
    );
}
pub use backstop_contract::{
    BackstopDataKey, BackstopEmissionsData, Client as BackstopClient, WASM as BACKSTOP_WASM,
};

pub fn create_backstop<'a>(e: &Env) -> (Address, BackstopClient<'a>) {
    let contract_id = Address::random(&e);
    e.register_contract_wasm(&contract_id, backstop_contract::WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}
