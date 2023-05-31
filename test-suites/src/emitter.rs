use soroban_sdk::{testutils::Address as _, Address, Env};

mod emitter_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/emitter.wasm");
}
pub use emitter_contract::{
    Client as EmitterClient, EmitterDataKey, EmitterError, WASM as EMITTER_WASM,
};

pub fn create_emitter<'a>(e: &Env) -> (Address, EmitterClient<'a>) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, emitter_contract::WASM);
    (contract_id.clone(), EmitterClient::new(e, &contract_id))
}
