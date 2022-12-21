#![cfg(test)]

use soroban_auth::{Identifier, Signature};
use soroban_sdk::{symbol, testutils::Accounts, Env, Status};

mod common;
use crate::common::{create_metadata, create_wasm_b_token, TokenError};

#[test]
fn test_xfer() {
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
    let transfer_amount: i128 = 20;
    b_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    b_token_client.with_source_account(&samwise).xfer(
        &Signature::Invoker,
        &0,
        &bombadil_id,
        &transfer_amount,
    );
    assert_eq!(b_token_client.balance(&bombadil_id), transfer_amount);
    assert_eq!(
        b_token_client.balance(&samwise_id),
        mint_amount - transfer_amount
    );
}

#[test]
fn test_xfer_ed25519() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (samwise_id, samwise_sign_1) = soroban_auth::testutils::ed25519::generate(&e);

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount: i128 = 100;
    let transfer_amount: i128 = 20;

    b_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    //create signature
    let nonce = b_token_client.nonce(&samwise_id);
    let sig1 = soroban_auth::testutils::ed25519::sign(
        &e,
        &samwise_sign_1,
        &b_token_client.contract_id,
        symbol!("xfer"),
        (&samwise_id, &nonce, &bombadil_id, &transfer_amount),
    );

    b_token_client.with_source_account(&bombadil).xfer(
        &sig1,
        &nonce,
        &bombadil_id,
        &transfer_amount,
    );
    assert_eq!(b_token_client.balance(&bombadil_id), transfer_amount);
    assert_eq!(
        b_token_client.balance(&samwise_id),
        mint_amount - transfer_amount
    );
}

#[test]
fn test_negative_xfer_panics() {
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
    let transfer_amount: i128 = -20;

    b_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    let result = b_token_client.with_source_account(&bombadil).try_xfer(
        &Signature::Invoker,
        &0,
        &bombadil_id,
        &transfer_amount,
    );
    assert_eq!(b_token_client.balance(&bombadil_id), 0);
    assert_eq!(b_token_client.balance(&samwise_id), mint_amount);
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
fn test_insufficient_balance_panics() {
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
    let transfer_amount: i128 = 200;

    b_token_client
        .with_source_account(&bombadil)
        .mint(&samwise_id, &mint_amount);

    let result = b_token_client.with_source_account(&bombadil).try_xfer(
        &Signature::Invoker,
        &0,
        &bombadil_id,
        &transfer_amount,
    );
    assert_eq!(b_token_client.balance(&bombadil_id), 0);
    assert_eq!(b_token_client.balance(&samwise_id), mint_amount);
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
