use common::create_b_token;
use soroban_sdk::{testutils::BytesN as _, Address, Bytes, BytesN, Env, IntoVal, Status};

mod common;
use crate::common::TokenError;

#[test]
fn test_init_b_token() {
    let e = Env::default();

    let (_b_token_id, b_token_client) = create_b_token(&e);

    let pool_id = BytesN::<32>::random(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let decimal: u32 = 7;
    let name: Bytes = "name".into_val(&e);
    let symbol: Bytes = "symbol".into_val(&e);

    let asset_id = BytesN::<32>::random(&e);
    let res_index: u32 = 3;

    // initialize token
    b_token_client.initialize(&pool, &decimal, &name, &symbol);
    assert_eq!(pool, b_token_client.pool());
    assert_eq!(decimal, b_token_client.decimals());
    assert_eq!(name, b_token_client.name());
    assert_eq!(symbol, b_token_client.symbol());

    // can't initialize a second time
    let result = b_token_client.try_initialize(&pool, &18, &name, &symbol);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from_contract_error(TokenError::AlreadyInitializedError as u32)
    );

    // initialize asset
    b_token_client.init_asset(&pool, &pool_id, &asset_id, &res_index);
    let asset_result = b_token_client.asset();
    assert_eq!(asset_id, asset_result.id);
    assert_eq!(res_index, asset_result.res_index);

    // can't initialize a second time
    let result = b_token_client.try_init_asset(&pool, &pool_id, &asset_id, &res_index);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from_contract_error(TokenError::AlreadyInitializedError as u32)
    );
}
