use soroban_sdk::{testutils::BytesN as _, BytesN, Env};

mod backstop_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/backstop_module.wasm"
    );
}
pub use backstop_contract::{BackstopDataKey, Client as BackstopClient, WASM as BACKSTOP_WASM};

pub fn create_backstop(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, backstop_contract::WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}
