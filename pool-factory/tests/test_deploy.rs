#![cfg(test)]

use soroban_sdk::{
    symbol,
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, BytesN, Env, IntoVal,
};

use crate::common::{create_wasm_pool_factory, generate_contract_id, lending_pool};
mod common;

#[test]
fn test_deploy() {
    let e = Env::default();
    let (_pool_factory_address, pool_factory_client) = create_wasm_pool_factory(&e);

    let wasm_hash = e.install_contract_wasm(lending_pool::WASM);
    pool_factory_client.initialize(&wasm_hash);

    let bombadil = Address::random(&e);

    let oracle = generate_contract_id(&e);
    // TODO: Verify this works when issues/14 is resolved
    let args = (bombadil, oracle).into_val(&e);
    let init_func = symbol!("initialize");

    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 100,
        network_id: Default::default(),
        base_reserve: 10,
    });
    let deployed_pool_address_1 = pool_factory_client.deploy(&init_func, &args);

    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 101,
        network_id: Default::default(),
        base_reserve: 10,
    });
    let deployed_pool_address_2 = pool_factory_client.deploy(&init_func, &args);

    let zero_address = BytesN::from_array(&e, &[0; 32]);

    assert_ne!(deployed_pool_address_1, zero_address);
    assert_ne!(deployed_pool_address_2, zero_address);
    assert_ne!(deployed_pool_address_1, deployed_pool_address_2);
    assert!(pool_factory_client.is_pool(&deployed_pool_address_1));
    assert!(pool_factory_client.is_pool(&deployed_pool_address_2));
    assert!(!pool_factory_client.is_pool(&zero_address));
}
