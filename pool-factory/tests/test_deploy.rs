#![cfg(test)]
use soroban_auth::Identifier;
use soroban_sdk::{
    symbol,
    testutils::{Accounts, Ledger, LedgerInfo},
    Bytes, BytesN, Env, IntoVal,
};
mod lending_pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm"
    );
}
mod common;
use crate::common::{create_wasm_pool_factory, generate_contract_id};

#[test]
fn test_deploy() {
    let e = Env::default();
    let (_pool_factory_address, pool_factory_client) = create_wasm_pool_factory(&e);
    pool_factory_client.initialize(&lending_pool::WASM.into_val(&e));

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil);
    let oracle = generate_contract_id(&e);
    let args = (bombadil_id, oracle).into_val(&e);
    let init_func = symbol!("initialize");

    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 100,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });
    let deployed_pool_address_1 = pool_factory_client.deploy(&init_func, &args);

    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 101,
        network_passphrase: Default::default(),
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
