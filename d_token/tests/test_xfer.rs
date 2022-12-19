#![cfg(test)]

use soroban_auth::Identifier;
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{create_metadata, create_wasm_d_token, DTokenError};

#[test]
fn test_xfer_from_admin() {
    let e = Env::default();

    // normally a contract would be the admin for the d_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let (d_token, d_token_client) = create_wasm_d_token(&e);
    let d_token_metadata = create_metadata(&e);

    d_token_client.init(&bombadil_id, &d_token_metadata);

    let mint_amount: i128 = 100;
    let transfer_amount: i128 = 20;

    d_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);
    d_token_client.with_source_account(&bombadil).xfer_from(
        &samwise_id,
        &bombadil_id,
        &transfer_amount,
    );
    assert_eq!(d_token_client.balance(&bombadil_id), transfer_amount);
    assert_eq!(
        d_token_client.balance(&samwise_id),
        mint_amount - transfer_amount
    );
}

#[test]
fn test_xfer_from_non_admin_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the d_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let sauron = e.accounts().generate_and_create();
    let sauron_id = Identifier::Account(sauron.clone());

    let (d_token, d_token_client) = create_wasm_d_token(&e);
    let d_token_metadata = create_metadata(&e);

    d_token_client.init(&bombadil_id, &d_token_metadata);

    let mint_amount: i128 = 100;
    let transfer_amount: i128 = 20;

    d_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    let result = d_token_client.with_source_account(&sauron).try_xfer_from(
        &samwise_id,
        &sauron_id,
        &transfer_amount,
    );
    assert_eq!(d_token_client.balance(&sauron_id), 0);
    assert_eq!(d_token_client.balance(&samwise_id), mint_amount);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, DTokenError::NotAuthorized),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(3)),
        },
    }
}

#[test]
fn test_negative_xfer_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the d_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let (d_token, d_token_client) = create_wasm_d_token(&e);
    let d_token_metadata = create_metadata(&e);

    d_token_client.init(&bombadil_id, &d_token_metadata);

    let mint_amount: i128 = 100;
    let transfer_amount: i128 = -20;

    d_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    let result = d_token_client.with_source_account(&bombadil).try_xfer_from(
        &samwise_id,
        &bombadil_id,
        &transfer_amount,
    );
    assert_eq!(d_token_client.balance(&bombadil_id), 0);
    assert_eq!(d_token_client.balance(&samwise_id), mint_amount);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, DTokenError::NegativeNumber),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(1)),
        },
    }
}

#[test]
fn test_insufficient_balance_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the d_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let (d_token, d_token_client) = create_wasm_d_token(&e);
    let d_token_metadata = create_metadata(&e);

    d_token_client.init(&bombadil_id, &d_token_metadata);

    let mint_amount: i128 = 100;
    let transfer_amount: i128 = 200;

    d_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    let result = d_token_client.with_source_account(&bombadil).try_xfer_from(
        &samwise_id,
        &bombadil_id,
        &transfer_amount,
    );
    assert_eq!(d_token_client.balance(&bombadil_id), 0);
    assert_eq!(d_token_client.balance(&samwise_id), mint_amount);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, DTokenError::BalanceError),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(5)),
        },
    }
}
