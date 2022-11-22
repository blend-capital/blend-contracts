#![cfg(test)]
use soroban_auth::Identifier;
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{create_mock_oracle, create_wasm_lending_pool, PoolError};

#[test]
fn test_set_status() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let (mock_oracle, _mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let _pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);
    pool_client.with_source_account(&bombadil).set_status(&0);
    assert_eq!(pool_client.status(), 0);

    pool_client.with_source_account(&bombadil).set_status(&2);
    assert_eq!(pool_client.status(), 2);

    pool_client.with_source_account(&bombadil).set_status(&1);
    assert_eq!(pool_client.status(), 1);
}

#[test]
fn test_set_status_not_admin_panic() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();

    let (mock_oracle, _mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let _pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);
    let result = pool_client.with_source_account(&sauron).try_set_status(&0);

    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::NotAuthorized),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(1)),
        },
    }
}
