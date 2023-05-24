use soroban_sdk::{testutils::Address as _, Address, Env};

mod mock_blend_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}
pub use mock_blend_oracle::{Client as MockOracleClient, OracleError};

pub fn create_mock_oracle<'a>(e: &Env) -> (Address, MockOracleClient<'a>) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, mock_blend_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, &contract_id))
}
