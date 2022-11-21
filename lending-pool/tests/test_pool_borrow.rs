#![cfg(test)]
use soroban_sdk::{BigInt, Env, testutils::Accounts, Status};
use soroban_auth::{Identifier, Signature};

mod common;
use crate::common::{create_wasm_lending_pool, create_mock_oracle, pool_helper, PoolError, TokenClient};

#[test]
fn test_pool_borrow_no_collateral_panics() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();
    let sauron_id = Identifier::Account(sauron.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);

    let (asset1_id, _, _) = pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    // borrow
    let borrow_amount = BigInt::from_u64(&e, 0_0000001);
    let result = pool_client.with_source_account(&sauron).try_borrow(&asset1_id, &borrow_amount, &sauron_id);
    match result {
        Ok(_) => assert!(false),
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidHf),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(3)),
        } 
    }
}

#[test]
fn test_pool_borrow_bad_hf_panics() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();
    let sauron_id = Identifier::Account(sauron.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);

    let (asset1_id, b_token1_id, _) = pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, asset1_id.clone());
    let b_token1_client = TokenClient::new(&e, b_token1_id.clone());
    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &sauron_id,
        &BigInt::from_i64(&e, 10_0000000),
    );
    asset1_client.with_source_account(&sauron).approve(
        &Signature::Invoker, 
        &BigInt::zero(&e), 
        &pool_id, 
        &BigInt::from_u64(&e, u64::MAX)
    );

    let minted_btokens = pool_client.with_source_account(&sauron).supply(&asset1_id, &BigInt::from_u64(&e, 1_0000000));
    assert_eq!(b_token1_client.balance(&sauron_id), minted_btokens);

    // borrow
    let borrow_amount = BigInt::from_u64(&e, 0_5358000); // 0.75 cf * 0.75 lf => 0.5625 / 1.05 hf min => 0.5357 max
    let result = pool_client.with_source_account(&sauron).try_borrow(&asset1_id, &borrow_amount, &sauron_id);
    match result {
        Ok(_) => {
            assert!(false);
        },
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidHf),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(3)),
        } 
    }
}

#[test]
fn test_pool_borrow_good_hf_borrows() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);

    let (asset1_id, b_token1_id, d_token1_id) = pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, asset1_id.clone());
    let b_token1_client = TokenClient::new(&e, b_token1_id.clone());
    let d_token1_client = TokenClient::new(&e, d_token1_id.clone());
    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &samwise_id,
        &BigInt::from_i64(&e, 10_0000000),
    );
    asset1_client.with_source_account(&samwise).approve(
        &Signature::Invoker, 
        &BigInt::zero(&e), 
        &pool_id, 
        &BigInt::from_u64(&e, u64::MAX)
    );

    let minted_btokens = pool_client.with_source_account(&samwise).supply(&asset1_id, &BigInt::from_u64(&e, 1_0000000));
    assert_eq!(b_token1_client.balance(&samwise_id), minted_btokens);

    // borrow
    let borrow_amount = BigInt::from_u64(&e, 0_5357000); // 0.75 cf * 0.75 lf => 0.5625 / 1.05 hf min => 0.5357 max
    let minted_dtokens = pool_client.with_source_account(&samwise).borrow(&asset1_id, &borrow_amount, &samwise_id);
    assert_eq!(asset1_client.balance(&samwise_id), BigInt::from_u64(&e, 10_0000000 - 1_0000000 + 0_5357000));
    assert_eq!(asset1_client.balance(&pool_id), BigInt::from_u64(&e, 1_0000000 - 0_5357000));
    assert_eq!(b_token1_client.balance(&samwise_id), minted_btokens);
    assert_eq!(d_token1_client.balance(&samwise_id), minted_dtokens);
}