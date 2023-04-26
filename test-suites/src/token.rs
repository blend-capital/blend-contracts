use soroban_sdk::{testutils::BytesN as _, Address, BytesN, Env, IntoVal};

mod token_contract {
    soroban_sdk::contractimport!(file = "../soroban_token_contract.wasm");
}
pub use token_contract::{Client as TokenClient, WASM as TOKEN_WASM};

pub fn create_stellar_token(e: &Env, admin: &Address) -> (BytesN<32>, TokenClient) {
    let contract_id = e.register_stellar_asset_contract(admin.clone());
    let client = TokenClient::new(e, &contract_id);
    (contract_id, client)
}

pub fn create_token(
    e: &Env,
    admin: &Address,
    decimals: u32,
    symbol: &str,
) -> (BytesN<32>, TokenClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, &contract_id);
    client.initialize(
        admin,
        &decimals,
        &"test token".into_val(e),
        &symbol.into_val(e),
    );
    (contract_id.clone(), client)
}
