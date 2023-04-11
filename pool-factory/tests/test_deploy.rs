#![cfg(test)]

use soroban_sdk::{
    storage::Storage,
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    unwrap::UnwrapOptimized,
    Address, BytesN, Env, IntoVal, Symbol,
};

use crate::common::{
    b_token, create_wasm_pool_factory, d_token, generate_contract_id,
    lending_pool::{self, PoolConfig, PoolDataKey},
};
mod common;

#[test]
fn test_deploy() {
    let e = Env::default();
    let (_pool_factory_address, pool_factory_client) = create_wasm_pool_factory(&e);

    let wasm_hash = e.install_contract_wasm(lending_pool::WASM);
    pool_factory_client.initialize(&wasm_hash);

    let bombadil = Address::random(&e);

    let oracle = generate_contract_id(&e);
    let backstop_id = generate_contract_id(&e);
    let backstop_address = Address::random(&e);
    let backstop_rate: u64 = 100000;
    let b_token_hash = e.install_contract_wasm(b_token::WASM);
    let d_token_hash = e.install_contract_wasm(d_token::WASM);

    // TODO: Verify this works when issues/14 is resolved
    let args = (
        bombadil.clone(),
        oracle.clone(),
        backstop_id.clone(),
        backstop_address.clone(),
        backstop_rate.clone(),
        b_token_hash.clone(),
        d_token_hash.clone(),
    )
        .into_val(&e);
    let init_func = Symbol::new(&e, "initialize");

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
    e.as_contract(&deployed_pool_address_1, || {
        let storage = e.storage();
        assert_eq!(
            storage
                .get::<_, Address>(&PoolDataKey::Admin)
                .unwrap()
                .unwrap(),
            bombadil.clone()
        );
        assert_eq!(
            storage
                .get::<_, BytesN<32>>(&PoolDataKey::Backstop)
                .unwrap()
                .unwrap(),
            backstop_id.clone()
        );
        assert_eq!(
            storage
                .get::<_, Address>(&PoolDataKey::BkstpAddr)
                .unwrap()
                .unwrap(),
            backstop_address.clone()
        );
        assert_eq!(
            storage
                .get::<_, PoolConfig>(&PoolDataKey::PoolConfig)
                .unwrap()
                .unwrap(),
            PoolConfig {
                oracle: oracle,
                bstop_rate: backstop_rate,
                status: 1
            }
        );
        assert_eq!(
            storage
                .get::<_, (BytesN<32>, BytesN<32>)>(&PoolDataKey::TokenHash)
                .unwrap()
                .unwrap(),
            (b_token_hash, d_token_hash)
        );
    });
    assert_ne!(deployed_pool_address_1, zero_address);
    assert_ne!(deployed_pool_address_2, zero_address);
    assert_ne!(deployed_pool_address_1, deployed_pool_address_2);
    assert!(pool_factory_client.is_pool(&deployed_pool_address_1));
    assert!(pool_factory_client.is_pool(&deployed_pool_address_2));
    assert!(!pool_factory_client.is_pool(&zero_address));
}
