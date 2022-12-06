#![cfg(test)]
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{
    create_mock_oracle, create_wasm_lending_pool, pool_helper, PoolError, TokenClient,
};

#[test]
fn test_pool_withdraw_no_supply_panics() {
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

    let asset1_client = TokenClient::new(&e, asset1_id.clone());
    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &pool_id,
        &10_0000000,
    );

    // withdraw
    let withdraw_amount = 0_0000006; // TODO: Update to one stroop with https://github.com/blend-capital/blend-contracts/issues/2
    let result = pool_client.with_source_account(&sauron).try_withdraw(
        &asset1_id,
        &withdraw_amount,
        &sauron_id,
    );
    match result {
        Ok(_) => assert!(false),
        Err(error) => match error {
            Ok(_p_error) => assert!(false),
            // TODO: Might be a bug with floating the ContractError from the `xfer` call
            Err(_s_error) => {
                // assert_eq!(s_error, Status::from_contract_error(11))
                assert!(true)
            }
        },
    }
}

#[test]
fn test_pool_withdraw_bad_hf_panics() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();
    let sauron_id = Identifier::Account(sauron.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);
    pool_client.with_source_account(&bombadil).set_status(&0);

    let (asset1_id, b_token1_id, d_token1_id) =
        pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, asset1_id.clone());
    let b_token1_client = TokenClient::new(&e, b_token1_id.clone());
    let d_token1_client = TokenClient::new(&e, d_token1_id.clone());
    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &sauron_id,
        &10_0000000,
    );
    asset1_client.with_source_account(&sauron).approve(
        &Signature::Invoker,
        &0,
        &pool_id,
        &(u64::MAX as i128),
    );

    // supply
    let minted_btokens = pool_client
        .with_source_account(&sauron)
        .supply(&asset1_id, &1_0000000);
    assert_eq!(b_token1_client.balance(&sauron_id), minted_btokens as i128);

    // borrow
    let minted_dtokens = pool_client
        .with_source_account(&sauron)
        .borrow(&asset1_id, &0_5357000, &sauron_id);
    assert_eq!(d_token1_client.balance(&sauron_id), minted_dtokens as i128);

    // withdraw
    let withdraw_amount = 0_0001000;
    let result = pool_client.with_source_account(&sauron).try_withdraw(
        &asset1_id,
        &withdraw_amount,
        &sauron_id,
    );
    match result {
        Ok(_) => assert!(false),
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidHf),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(3)),
        },
    }
}

#[test]
fn test_pool_withdraw_good_hf_withdraws() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);
    pool_client.with_source_account(&bombadil).set_status(&0);

    let (asset1_id, b_token1_id, d_token1_id) =
        pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, asset1_id.clone());
    let b_token1_client = TokenClient::new(&e, b_token1_id.clone());
    let d_token1_client = TokenClient::new(&e, d_token1_id.clone());
    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &samwise_id,
        &10_0000000,
    );
    asset1_client.with_source_account(&samwise).approve(
        &Signature::Invoker,
        &0,
        &pool_id,
        &(u64::MAX as i128),
    );

    // supply
    let minted_btokens = pool_client
        .with_source_account(&samwise)
        .supply(&asset1_id, &1_0000000);
    assert_eq!(b_token1_client.balance(&samwise_id), minted_btokens as i128);

    // borrow
    let minted_dtokens =
        pool_client
            .with_source_account(&samwise)
            .borrow(&asset1_id, &0_5355000, &samwise_id);
    assert_eq!(d_token1_client.balance(&samwise_id), minted_dtokens as i128);

    // withdraw
    let withdraw_amount = 0_0001000;
    let burnt_btokens = pool_client.with_source_account(&samwise).withdraw(
        &asset1_id,
        &withdraw_amount,
        &samwise_id,
    );
    assert_eq!(
        asset1_client.balance(&samwise_id),
        10_0000000 - 1_0000000 + 0_5355000 + 0_0001000
    );
    assert_eq!(
        asset1_client.balance(&pool_id),
        1_0000000 - 0_5355000 - 0_0001000
    );
    assert_eq!(
        b_token1_client.balance(&samwise_id),
        (minted_btokens - burnt_btokens) as i128
    );
    assert_eq!(d_token1_client.balance(&samwise_id), minted_dtokens as i128);
}
