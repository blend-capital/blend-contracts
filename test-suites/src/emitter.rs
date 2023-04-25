mod emitter_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/emitter.wasm");
}
pub use emitter_contract::{
    Client as EmitterClient, EmitterDataKey, EmitterError, WASM as EMITTER_WASM,
};

use crate::helpers::generate_contract_id;
pub fn create_wasm_emitter(e: &soroban_sdk::Env) -> (soroban_sdk::BytesN<32>, EmitterClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, emitter_contract::WASM);
    (contract_id.clone(), EmitterClient::new(e, &contract_id))
}
