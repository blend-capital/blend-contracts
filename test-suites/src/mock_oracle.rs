mod mock_blend_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}
pub use mock_blend_oracle::{Client as MockOracleClient, OracleError};
use soroban_sdk::{BytesN, Env};

use crate::helpers::generate_contract_id;

pub fn create_mock_oracle(e: &Env) -> (BytesN<32>, MockOracleClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, mock_blend_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, &contract_id))
}
