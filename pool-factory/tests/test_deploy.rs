#![cfg(test)]
use soroban_auth::Identifier;
use soroban_sdk::{symbol, testutils::Accounts, Bytes, BytesN, Env, IntoVal};
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

    let wasm = lending_pool::WASM.into_val(&e);
    let salt1 = Bytes::from_array(&e, &[0; 32]);
    let salt2 = Bytes::from_array(&e, &[1; 32]);

    let admin = e.accounts().generate_and_create();
    let admin_id = Identifier::Account(admin);
    let oracle = generate_contract_id(&e);
    let args = (admin_id, oracle).into_val(&e);
    let init_func = symbol!("initialize");

    let deployed_pool_address_1 = pool_factory_client.deploy(&wasm, &salt1, &init_func, &args);
    let deployed_pool_address_2 = pool_factory_client.deploy(&wasm, &salt2, &init_func, &args);
    let zero_address = BytesN::from_array(&e, &[0; 32]);

    assert_ne!(deployed_pool_address_1, zero_address);
    assert_ne!(deployed_pool_address_2, zero_address);
    assert_ne!(deployed_pool_address_1, deployed_pool_address_2);
    assert!(pool_factory_client.is_deploy(&deployed_pool_address_1));
    assert!(pool_factory_client.is_deploy(&deployed_pool_address_2));
    assert!(!pool_factory_client.is_deploy(&zero_address));
}
