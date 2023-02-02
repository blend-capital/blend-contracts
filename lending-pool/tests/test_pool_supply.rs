#![cfg(test)]
use cast::i128;
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{
    create_mock_oracle, create_wasm_lending_pool, generate_contract_id, pool_helper, PoolError,
    TokenClient,
};

#[test]
fn test_pool_supply_on_ice() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();
    let sauron_id = Identifier::Account(sauron.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_address = generate_contract_id(&e);
    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(
        &bombadil_id,
        &mock_oracle,
        &backstop_address,
        &0_200_000_000,
    );
    pool_client.with_source_account(&bombadil).set_status(&1);

    let (asset1_id, _b_token1_id, _) =
        pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, asset1_id.clone());

    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &sauron_id,
        &10_0000000,
    );
    asset1_client.with_source_account(&sauron).incr_allow(
        &Signature::Invoker,
        &0,
        &pool_id,
        &i128(u64::MAX),
    );

    let result = pool_client
        .with_source_account(&sauron)
        .try_supply(&asset1_id, &1_0000000);

    match result {
        Ok(_) => assert!(true),
        Err(_) => assert!(false),
    }
}

#[test]
fn test_pool_supply_frozen_panics() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();
    let sauron_id = Identifier::Account(sauron.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_address = generate_contract_id(&e);
    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(
        &bombadil_id,
        &mock_oracle,
        &backstop_address,
        &0_200_000_000,
    );
    pool_client.with_source_account(&bombadil).set_status(&2);

    let (asset1_id, _b_token1_id, _) =
        pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, asset1_id.clone());

    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &sauron_id,
        &10_0000000,
    );
    asset1_client.with_source_account(&sauron).incr_allow(
        &Signature::Invoker,
        &0,
        &pool_id,
        &i128(u64::MAX),
    );

    let result = pool_client
        .with_source_account(&sauron)
        .try_supply(&asset1_id, &1_0000000);

    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidPoolStatus),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(4)),
        },
    }
}
