use soroban_sdk::{testutils::Address as _, Address, Env};

mod backstop_contract_wasm {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/backstop.wasm");
}
use backstop::{BackstopClient, BackstopContract};

pub fn create_backstop<'a>(e: &Env, wasm: bool) -> (Address, BackstopClient<'a>) {
    let contract_id = Address::generate(e);
    if wasm {
        e.register_contract_wasm(&contract_id, backstop_contract_wasm::WASM);
    } else {
        e.register_contract(&contract_id, BackstopContract {});
    }
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}
