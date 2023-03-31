#![cfg(test)]
use soroban_sdk::{testutils::Address as AddressTestTrait, Address, Env, Status};

mod common;
use crate::common::{
    create_mock_oracle, create_wasm_lending_pool, generate_contract_id, PoolError, pool_helper,
};

#[test]
fn test_set_status() {
    let e = Env::default();

    let bombadil = Address::random(&e);

    let (oracle_id, _) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (_, pool_client) = create_wasm_lending_pool(&e);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    pool_client.set_status(&bombadil, &0);
    assert_eq!(pool_client.status(), 0);

    pool_client.set_status(&bombadil, &2);
    assert_eq!(pool_client.status(), 2);

    pool_client.set_status(&bombadil, &1);
    assert_eq!(pool_client.status(), 1);
}

#[test]
fn test_set_status_not_admin_panic() {
    let e = Env::default();

    let bombadil = Address::random(&e);

    let sauron = Address::random(&e);

    let (oracle_id, _) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (_, pool_client) = create_wasm_lending_pool(&e);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let result = pool_client.try_set_status(&sauron, &0);

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
