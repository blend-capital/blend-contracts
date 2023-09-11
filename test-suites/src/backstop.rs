use soroban_sdk::{testutils::Address as _, Address, Env};

mod backstop_contract_wasm {
    soroban_sdk::contractimport!(file = "./wasm/backstop_module.wasm");
}
pub use backstop_contract_wasm::{Client as BackstopModuleClient, Contract as BackstopModule};

pub fn create_backstop<'a>(e: &Env, wasm: bool) -> (Address, BackstopModuleClient<'a>) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, backstop_contract_wasm::WASM);

    (
        contract_id.clone(),
        BackstopModuleClient::new(e, &contract_id),
    )
}
