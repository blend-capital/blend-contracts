use soroban_sdk::{contractimport, Address, BytesN, Env};

contractimport!(file = "../soroban_token_contract.wasm");

pub use Client as TokenClient;

pub fn create_token(e: &Env, admin: &Address) -> (BytesN<32>, TokenClient) {
    let contract_id = e.register_stellar_asset_contract(admin.clone());
    let client = TokenClient::new(e, &contract_id);
    (contract_id, client)
}
