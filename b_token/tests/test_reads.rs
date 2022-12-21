#![cfg(test)]
use soroban_auth::Identifier;
use soroban_sdk::{testutils::Accounts, Env};

mod common;
use crate::common::{create_metadata, create_wasm_b_token};

#[test]
fn test_reads() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    assert_eq!(b_token_client.decimals(), b_token_metadata.decimals);
    assert_eq!(b_token_client.name(), b_token_metadata.name);
    assert_eq!(b_token_client.symbol(), b_token_metadata.symbol);
}

#[test]
fn test_nonzero_balance() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    let mint_amount = 100;
    b_token_client
        .with_source_account(&bombadil)
        .mint(&bombadil_id, &mint_amount);
    assert_eq!(b_token_client.balance(&bombadil_id), 100);
}

#[test]
fn test_zero_balance() {
    let e = Env::default();

    // normally a contract would be the admin for the b_token, but since we can't call functions as
    // a contract in tests yet, we'll use an account for now. TODO: switch account to contract
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (b_token, b_token_client) = create_wasm_b_token(&e);
    let b_token_metadata = create_metadata(&e);

    b_token_client.init(&bombadil_id, &b_token_metadata);

    assert_eq!(b_token_client.balance(&bombadil_id), 0);
}
