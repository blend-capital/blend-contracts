use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    Address, BytesN, Env, IntoVal, Status,
};

mod common;
use crate::common::{
    create_b_token, create_lending_pool, DTokenClient, TokenError,
};

fn create_and_init_b_token(
    e: &Env,
    pool: &Address,
    pool_id: &BytesN<32>,
    index: &u32,
) -> (BytesN<32>, DTokenClient) {
    let (b_token_id, b_token_client) = create_b_token(e);
    b_token_client.initialize(pool, &7, &"name".into_val(e), &"symbol".into_val(e));
    b_token_client.init_asset(pool, pool_id, &BytesN::<32>::random(&e), index);
    (b_token_id, b_token_client)
}

#[test]
fn test_mint() {
    let e = Env::default();

    let pool_id = BytesN::<32>::random(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let (_, b_token_client) = create_and_init_b_token(&e, &pool, &pool_id, &2);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    b_token_client.mint(&pool, &samwise, &123456789);
    assert_eq!(123456789, b_token_client.balance(&samwise));

    // verify only pool can mint
    let result = b_token_client.try_mint(&sauron, &samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify can't mint a negative number
    let result = b_token_client.try_mint(&pool, &samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_clawback() {
    let e = Env::default();

    let pool_id = BytesN::<32>::random(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let (_, b_token_client) = create_and_init_b_token(&e, &pool, &pool_id, &2);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    b_token_client.mint(&pool, &samwise, &123456789);
    assert_eq!(123456789, b_token_client.balance(&samwise));

    b_token_client.clawback(&pool, &samwise, &23456789);
    assert_eq!(100000000, b_token_client.balance(&samwise));

    // verify only pool can clawback
    let result = b_token_client.try_clawback(&sauron, &samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify can't clawback a negative number
    let result = b_token_client.try_clawback(&pool, &samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_incr_allow() {
    let e = Env::default();

    let res_index = 7;
    let pool_id = BytesN::<32>::random(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let (_, b_token_client) = create_and_init_b_token(&e, &pool, &pool_id, &res_index);

    let samwise = Address::random(&e);
    let spender = Address::random(&e);

    // verify happy path
    b_token_client.incr_allow(&samwise, &spender, &123456789);
    assert_eq!(123456789, b_token_client.allowance(&samwise, &spender));

    // verify negative balance cannot be used
    let result = b_token_client.try_incr_allow(&samwise, &spender, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_decr_allow() {
    let e = Env::default();

    let res_index = 7;
    let pool_id = BytesN::<32>::random(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let (_, b_token_client) = create_and_init_b_token(&e, &pool, &pool_id, &res_index);

    let samwise = Address::random(&e);
    let spender = Address::random(&e);

    // verify happy path
    b_token_client.incr_allow(&samwise, &spender, &123456789);
    b_token_client.decr_allow(&samwise, &spender, &23456789);
    assert_eq!(100000000, b_token_client.allowance(&samwise, &spender));

    // verify negative balance cannot be used
    let result = b_token_client.try_decr_allow(&samwise, &spender, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_xfer() {
    let e = Env::default();

    let res_index = 7;
    let (pool_id, pool_client) = create_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let (_, b_token_client) = create_and_init_b_token(&e, &pool, &pool_id, &res_index);

    let samwise = Address::random(&e);
    let frodo = Address::random(&e);

    // verify happy path
    b_token_client.mint(&pool, &samwise, &123456789);
    assert_eq!(123456789, b_token_client.balance(&samwise));
    pool_client.set_collat(&samwise, &res_index, &false);

    b_token_client.xfer(&samwise, &frodo, &23456789);
    assert_eq!(100000000, b_token_client.balance(&samwise));
    assert_eq!(23456789, b_token_client.balance(&frodo));

    // verify collateralized balance cannot transfer
    pool_client.set_collat(&frodo, &res_index, &true);
    let result = b_token_client.try_xfer(&frodo, &samwise, &1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify negative balance cannot be used
    let result = b_token_client.try_xfer(&samwise, &frodo, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_xfer_from() {
    let e = Env::default();

    let res_index = 7;
    let (pool_id, pool_client) = create_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let (_, b_token_client) = create_and_init_b_token(&e, &pool, &pool_id, &res_index);

    let samwise = Address::random(&e);
    let frodo = Address::random(&e);
    let spender = Address::random(&e);

    // verify happy path
    b_token_client.mint(&pool, &samwise, &123456789);
    assert_eq!(123456789, b_token_client.balance(&samwise));
    pool_client.set_collat(&samwise, &res_index, &false);

    b_token_client.incr_allow(&samwise, &spender, &223456789);
    b_token_client.xfer_from(&spender, &samwise, &frodo, &23456789);
    assert_eq!(100000000, b_token_client.balance(&samwise));
    assert_eq!(23456789, b_token_client.balance(&frodo));
    assert_eq!(200000000, b_token_client.allowance(&samwise, &spender));

    // verify negative balance cannot be used
    let result = b_token_client.try_xfer_from(&spender, &samwise, &frodo, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );

    // verify collateralized balance cannot transfer
    pool_client.set_collat(&samwise, &res_index, &true);
    let result = b_token_client.try_xfer_from(&spender, &samwise, &frodo, &1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );
}
