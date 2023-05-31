use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    vec,
    xdr::ScHostAuthErrorCode,
    Address, Env, IntoVal, Status, Symbol,
};

mod common;
use crate::common::{create_d_token, DTokenClient, TokenError};

fn create_and_init_d_token<'a>(e: &'a Env, pool_address: &Address) -> (Address, DTokenClient<'a>) {
    let (d_token_id, d_token_client) = create_d_token(e);
    d_token_client.initialize(pool_address, &7, &"name".into_val(e), &"symbol".into_val(e));
    d_token_client.initialize_asset(pool_address, &Address::random(e), &3);
    (d_token_id, d_token_client)
}

#[test]
fn test_mint() {
    let e = Env::default();
    e.mock_all_auths();

    let pool_address = Address::random(&e);
    let (d_token_id, d_token_client) = create_and_init_d_token(&e, &pool_address);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    d_token_client.mint(&samwise, &123456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            pool_address.clone(),
            d_token_id.clone(),
            Symbol::new(&e, "mint"),
            vec![&e, samwise.clone().to_raw(), 123456789_i128.into_val(&e)]
        )
    );
    assert_eq!(123456789, d_token_client.balance(&samwise));

    // verify only pool_address can mint
    let result = d_token_client
        .mock_auths(&[MockAuth {
            address: &sauron,
            nonce: 0,
            invoke: &MockAuthInvoke {
                contract: &d_token_id,
                fn_name: "mint",
                args: vec![&e, samwise.to_raw(), 2_i128.into_val(&e)],
                sub_invokes: &[],
            },
        }])
        .try_mint(&samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(ScHostAuthErrorCode::NotAuthorized)
    );
    e.mock_all_auths();
    // verify can't mint a negative number
    let result = d_token_client.try_mint(&samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_clawback() {
    let e = Env::default();
    e.mock_all_auths();

    let pool_address = Address::random(&e);
    let (d_token_id, d_token_client) = create_and_init_d_token(&e, &pool_address);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    d_token_client.mint(&samwise, &123456789);
    assert_eq!(123456789, d_token_client.balance(&samwise));

    d_token_client.clawback(&samwise, &23456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            pool_address.clone(),
            d_token_id.clone(),
            Symbol::new(&e, "clawback"),
            vec![&e, samwise.clone().to_raw(), 23456789_i128.into_val(&e)]
        )
    );
    assert_eq!(100000000, d_token_client.balance(&samwise));

    // verify only pool_address can clawback
    let result = d_token_client
        .mock_auths(&[MockAuth {
            address: &sauron,
            nonce: 0,
            invoke: &MockAuthInvoke {
                contract: &d_token_id,
                fn_name: "clawback",
                args: vec![&e, samwise.to_raw(), 2_i128.into_val(&e)],
                sub_invokes: &[],
            },
        }])
        .try_clawback(&samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(ScHostAuthErrorCode::NotAuthorized)
    );

    // verify can't clawback a negative number
    e.mock_all_auths();
    let result = d_token_client.try_clawback(&samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_transfer_from() {
    let e = Env::default();
    e.mock_all_auths();

    let pool_address = Address::random(&e);
    let (d_token_id, d_token_client) = create_and_init_d_token(&e, &pool_address);

    let samwise = Address::random(&e);
    let frodo = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    d_token_client.mint(&samwise, &123456789);
    assert_eq!(123456789, d_token_client.balance(&samwise));

    d_token_client.transfer_from(&pool_address, &samwise, &frodo, &23456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            pool_address.clone(),
            d_token_id.clone(),
            Symbol::new(&e, "transfer_from"),
            vec![
                &e,
                pool_address.clone().to_raw(),
                samwise.clone().to_raw(),
                frodo.clone().to_raw(),
                23456789_i128.into_val(&e)
            ]
        )
    );
    assert_eq!(100000000, d_token_client.balance(&samwise));
    assert_eq!(23456789, d_token_client.balance(&frodo));

    // verify only pool_address can transfer_from
    let result = d_token_client
        .mock_auths(&[MockAuth {
            address: &sauron,
            nonce: 0,
            invoke: &MockAuthInvoke {
                contract: &d_token_id,
                fn_name: "try_transfer_from",
                args: vec![&e, samwise.to_raw(), frodo.to_raw(), 2_i128.into_val(&e)],
                sub_invokes: &[],
            },
        }])
        .try_transfer_from(&sauron, &samwise, &frodo, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify can't transfer_from a negative number
    e.mock_all_auths();
    let result = d_token_client.try_transfer_from(&pool_address, &samwise, &frodo, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}
