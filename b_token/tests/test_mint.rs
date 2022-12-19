#![cfg(test)]
use soroban_auth::Identifier;
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{create_metadata, create_wasm_b_token, TokenError};

#[test]
fn test_mint_from_admin() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());
    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount: i128 = 100;
    println!("gonna mint");
    b_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    assert_eq!(b_token_client.balance(&samwise_id), mint_amount);
}

#[test]
fn test_mint_from_non_admin_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount: i128 = 100;

    let result = b_token_client
        .with_source_account(&sauron)
        .try_mint(&bombadil_id, &mint_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), 0);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, TokenError::NotAuthorized),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(3)),
        },
    }
}

//TODO: test currently failing, unsure why
#[test]
fn test_overflow_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);
    let u64_max: u64 = u64::MAX;
    let mint_amount: i128 = u64_max as i128;
    assert_eq!(b_token_client.balance(&bombadil_id), 0);
    b_token_client
        .with_source_account(&bombadil)
        .mint(&bombadil_id, &mint_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), mint_amount);
    let result = b_token_client
        .with_source_account(&bombadil)
        .try_mint(&bombadil_id, &100);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, TokenError::OverflowError),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(5)),
        },
    }
    assert_eq!(b_token_client.balance(&bombadil_id), mint_amount);
}

#[test]
fn test_negative_mint_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount: i128 = -10;

    let result = b_token_client
        .with_source_account(&bombadil)
        .try_mint(&bombadil_id, &mint_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), 0);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, TokenError::NegativeNumber),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(1)),
        },
    }
}
