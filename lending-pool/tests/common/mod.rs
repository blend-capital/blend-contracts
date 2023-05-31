use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal};

// Generics

mod token {
    soroban_sdk::contractimport!(file = "../soroban_token_contract.wasm");
}
pub use token::Client as TokenClient;

mod b_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}
pub use b_token::{Client as BlendTokenClient, WASM as B_TOKEN_WASM};

mod d_token {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/release/b_token.wasm");
}
pub use d_token::WASM as D_TOKEN_WASM;

mod pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm"
    );
}
pub use pool::{
    AuctionData, Client as PoolClient, LiquidationMetadata, PoolError, ReserveConfig, ReserveData,
    ReserveEmissionMetadata, ReserveMetadata,
};

mod backstop {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/backstop_module.wasm"
    );
}
pub use backstop::Client as BackstopClient;

mod mock_blend_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}
pub use mock_blend_oracle::{Client as MockOracleClient, OracleError};

mod mock_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory::Client as MockPoolFactoryClient;

pub fn create_token<'a>(e: &Env, admin: &Address) -> (Address, TokenClient<'a>) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, token::WASM);
    let client = TokenClient::new(e, &contract_id);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_id, client)
}

pub fn create_token_from_id<'a>(
    e: &Env,
    contract_id: &Address,
    admin: &Address,
) -> TokenClient<'a> {
    e.register_contract_wasm(contract_id, token::WASM);
    let client = TokenClient::new(e, contract_id);
    client.initialize(&admin, &7, &"unit".into_val(e), &"test".into_val(e));
    client
}

pub fn create_wasm_lending_pool(e: &Env) -> (Address, PoolClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, pool::WASM);
    (contract_id.clone(), PoolClient::new(e, &contract_id))
}

pub fn create_backstop(e: &Env) -> (Address, BackstopClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, backstop::WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}

pub fn create_mock_oracle(e: &Env) -> (Address, MockOracleClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, mock_blend_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, &contract_id))
}

pub fn create_mock_pool_factory(e: &Env) -> (Address, MockPoolFactoryClient) {
    let contract_id = Address::random(e);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    (
        contract_id.clone(),
        MockPoolFactoryClient::new(e, &contract_id),
    )
}

// Contract specific test functions

pub mod pool_helper;
