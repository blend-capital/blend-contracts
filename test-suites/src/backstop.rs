mod backstop_contract {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/backstop_module.wasm"
    );
}
pub use backstop_contract::{BackstopDataKey, Client as BackstopClient, WASM as BACKSTOP_WASM};
use soroban_sdk::{BytesN, Env};

use crate::helpers::generate_contract_id;

pub fn create_backstop(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, backstop_contract::WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}
