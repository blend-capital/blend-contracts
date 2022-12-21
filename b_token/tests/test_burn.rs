#![cfg(test)]

use soroban_auth::Identifier;
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{create_metadata, create_wasm_b_token, TokenError};

#[test]
fn test_burn_from_admin() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount: i128 = 100;
    let burn_amount: i128 = 40;

    b_token_client
        .with_source_account(&bombadil)
        .mint(&bombadil_id, &mint_amount);
    b_token_client
        .with_source_account(&bombadil)
        .burn(&bombadil_id, &burn_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), 60);
}

#[test]
fn test_burn_from_non_admin_panics() {
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
    let burn_amount: i128 = 40;

    b_token_client
        .with_source_account(&bombadil)
        .mint(&bombadil_id, &mint_amount);
    let result = b_token_client
        .with_source_account(&sauron)
        .try_burn(&bombadil_id, &burn_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), 100);

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

#[test]
fn test_negative_burn_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount: i128 = 100;
    let burn_amount: i128 = -40;

    b_token_client
        .with_source_account(&bombadil)
        .mint(&bombadil_id, &mint_amount);
    let result = b_token_client
        .with_source_account(&bombadil)
        .try_burn(&bombadil_id, &burn_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), 100);

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

#[test]
fn test_insufficient_balance_burn_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount: i128 = 100;
    let burn_amount: i128 = 400;

    b_token_client
        .with_source_account(&bombadil)
        .mint(&bombadil_id, &mint_amount);
    let result = b_token_client
        .with_source_account(&bombadil)
        .try_burn(&bombadil_id, &burn_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), 100);

    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, TokenError::BalanceError),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(5)),
        },
    }
}
