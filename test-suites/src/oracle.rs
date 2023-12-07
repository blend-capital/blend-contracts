use soroban_sdk::{testutils::Address as _, Address, Env};

use sep_40_oracle::testutils::{MockPriceOracleClient, MockPriceOracleWASM};

pub fn create_mock_oracle<'a>(e: &Env) -> (Address, MockPriceOracleClient<'a>) {
    let contract_id = Address::generate(e);
    e.register_contract_wasm(&contract_id, MockPriceOracleWASM);
    (
        contract_id.clone(),
        MockPriceOracleClient::new(e, &contract_id),
    )
}
