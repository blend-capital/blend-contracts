use soroban_sdk::{testutils::BytesN as _, BytesN, Env};

mod emitter_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/emitter.wasm");
}
pub use emitter_contract::{
    Client as EmitterClient, EmitterDataKey, EmitterError, WASM as EMITTER_WASM,
};

pub fn create_emitter(e: &Env) -> (BytesN<32>, EmitterClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, emitter_contract::WASM);
    (contract_id.clone(), EmitterClient::new(e, &contract_id))
}
