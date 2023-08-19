use soroban_sdk::{testutils::Address as _, Address, Env};

mod backstop_contract_wasm {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/backstop_module.wasm"
    );
}
use backstop_module::{BackstopModule, BackstopModuleClient};

pub fn create_backstop<'a>(e: &Env, wasm: bool) -> (Address, BackstopModuleClient<'a>) {
    let contract_id = Address::random(e);
    if wasm {
        e.register_contract_wasm(&contract_id, backstop_contract_wasm::WASM);
    } else {
        e.register_contract(&contract_id, BackstopModule {});
    }
    (
        contract_id.clone(),
        BackstopModuleClient::new(e, &contract_id),
    )
}
