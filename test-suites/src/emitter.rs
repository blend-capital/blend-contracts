use soroban_sdk::{testutils::Address as _, Address, Env};

mod emitter_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/emitter.wasm");
}

use emitter::{Emitter, EmitterClient};

pub fn create_emitter<'a>(e: &Env, wasm: bool) -> (Address, EmitterClient<'a>) {
    let contract_id = Address::random(e);
    if wasm {
        e.register_contract_wasm(&contract_id, emitter_contract::WASM);
    } else {
        e.register_contract(&contract_id, Emitter {});
    }
    (contract_id.clone(), EmitterClient::new(e, &contract_id))
}
