use common::create_d_token;
use soroban_sdk::{
    testutils::{Address as _},
    Address, Env, Status, Bytes, IntoVal,
};

mod common;
use crate::common::{
    TokenError
};

#[test]
fn test_init_d_token() {
    let e = Env::default();

    let (_d_token_id, d_token_client) = create_d_token(&e);

    let pool = Address::random(&e);
    let decimal: u32 = 7;
    let name: Bytes = "name".into_val(&e);
    let symbol: Bytes = "symbol".into_val(&e);

    let asset = Address::random(&e);
    let res_index: u32 = 3;
    
    // initialize token
    d_token_client.initialize(&pool, &decimal, &name, &symbol);
    assert_eq!(pool, d_token_client.pool());
    assert_eq!(decimal, d_token_client.decimals());
    assert_eq!(name, d_token_client.name());
    assert_eq!(symbol, d_token_client.symbol());

    // can't initialize a second time
    let result = d_token_client.try_initialize(&pool, &18, &name, &symbol);
    assert_eq!(result.unwrap_err().unwrap(), Status::from_contract_error(TokenError::AlreadyInitializedError as u32));

    // initialize asset
    d_token_client.init_asset(&pool, &asset, &res_index);
    let asset_result = d_token_client.asset();
    assert_eq!(asset, asset_result.address);
    assert_eq!(res_index, asset_result.res_index);

    // can't initialize a second time
    let result = d_token_client.try_init_asset(&pool, &asset, &res_index);
    assert_eq!(result.unwrap_err().unwrap(), Status::from_contract_error(TokenError::AlreadyInitializedError as u32));
}