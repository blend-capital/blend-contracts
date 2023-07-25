use soroban_sdk::{testutils::Address as _, Address, Env};

mod mock_blend_oracle_wasm {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}

use mock_oracle::{MockOracle, MockOracleClient};

pub fn create_mock_oracle<'a>(e: &Env, wasm: bool) -> (Address, MockOracleClient<'a>) {
    let contract_id = Address::random(e);
    if wasm {
        e.register_contract_wasm(&contract_id, mock_blend_oracle_wasm::WASM);
    } else {
        e.register_contract(&contract_id, MockOracle {});
    }
    (contract_id.clone(), MockOracleClient::new(e, &contract_id))
}
