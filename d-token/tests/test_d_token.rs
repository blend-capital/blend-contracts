use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, Status};

mod common;
use crate::common::{create_d_token, DTokenClient, TokenError};

fn create_and_init_d_token(e: &Env, pool: &Address) -> DTokenClient {
    let (_, d_token_client) = create_d_token(e);
    d_token_client.initialize(pool, &7, &"name".into_val(e), &"symbol".into_val(e));
    d_token_client.init_asset(pool, &Address::random(&e), &3);
    d_token_client
}

#[test]
fn test_mint() {
    let e = Env::default();

    let pool = Address::random(&e);
    let d_token_client = create_and_init_d_token(&e, &pool);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    d_token_client.mint(&pool, &samwise, &123456789);
    assert_eq!(123456789, d_token_client.balance(&samwise));

    // verify only pool can mint
    let result = d_token_client.try_mint(&sauron, &samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify can't mint a negative number
    let result = d_token_client.try_mint(&pool, &samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_clawback() {
    let e = Env::default();

    let pool = Address::random(&e);
    let d_token_client = create_and_init_d_token(&e, &pool);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    d_token_client.mint(&pool, &samwise, &123456789);
    assert_eq!(123456789, d_token_client.balance(&samwise));

    d_token_client.clawback(&pool, &samwise, &23456789);
    assert_eq!(100000000, d_token_client.balance(&samwise));

    // verify only pool can clawback
    let result = d_token_client.try_clawback(&sauron, &samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify can't clawback a negative number
    let result = d_token_client.try_clawback(&pool, &samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_xfer_from() {
    let e = Env::default();

    let pool = Address::random(&e);
    let d_token_client = create_and_init_d_token(&e, &pool);

    let samwise = Address::random(&e);
    let frodo = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    d_token_client.mint(&pool, &samwise, &123456789);
    assert_eq!(123456789, d_token_client.balance(&samwise));

    d_token_client.xfer_from(&pool, &samwise, &frodo, &23456789);
    assert_eq!(100000000, d_token_client.balance(&samwise));
    assert_eq!(23456789, d_token_client.balance(&frodo));

    // verify only pool can xfer_from
    let result = d_token_client.try_xfer_from(&sauron, &samwise, &frodo, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify can't xfer_from a negative number
    let result = d_token_client.try_xfer_from(&pool, &samwise, &frodo, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}
