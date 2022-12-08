#![cfg(test)]

use soroban_auth::Identifier;
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{create_metadata, create_wasm_d_token, DTokenError};

#[test]
fn test_mint_from_admin() {
    let e = Env::default();

    // normally a contract would be the admin for the d_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (d_token, d_token_client) = create_wasm_d_token(&e);
    let d_token_metadata = create_metadata(&e);

    d_token_client.init(&bombadil_id, &d_token_metadata);

    let mint_amount: i128 = 100;

    d_token_client
        .with_source_account(&bombadil)
        .mint(&bombadil_id, &mint_amount);
    assert_eq!(d_token_client.balance(&bombadil_id), mint_amount);
}

#[test]
fn test_mint_from_non_admin_panics() {
    let e = Env::default();

    // normally a contract would be the admin for the d_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();

    let (d_token, d_token_client) = create_wasm_d_token(&e);
    let d_token_metadata = create_metadata(&e);

    d_token_client.init(&bombadil_id, &d_token_metadata);

    let mint_amount: i128 = 100;

    let result = d_token_client
        .with_source_account(&sauron)
        .try_mint(&bombadil_id, &mint_amount);
    assert_eq!(d_token_client.balance(&bombadil_id), 0);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, DTokenError::NotAuthorized),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(4)),
        },
    }
}
