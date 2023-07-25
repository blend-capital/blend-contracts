#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Events},
    vec, Address, BytesN, Env, IntoVal, Symbol,
};

use crate::{PoolFactory, PoolFactoryClient, PoolInitMeta};

mod lending_pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/lending_pool.wasm"
    );
}

fn create_pool_factory(e: &Env) -> (Address, PoolFactoryClient) {
    let contract_id = e.register_contract(None, PoolFactory {});
    (contract_id.clone(), PoolFactoryClient::new(e, &contract_id))
}

#[test]
fn test_pool_factory() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths();
    let (pool_factory_address, pool_factory_client) = create_pool_factory(&e);

    let wasm_hash = e.deployer().upload_contract_wasm(lending_pool::WASM);

    let bombadil = Address::random(&e);

    let oracle = Address::random(&e);
    let backstop_id = Address::random(&e);
    let backstop_rate: u64 = 100000;
    let blnd_id = Address::random(&e);
    let usdc_id = Address::random(&e);

    let pool_init_meta = PoolInitMeta {
        backstop: backstop_id.clone(),
        pool_hash: wasm_hash.clone(),
        blnd_id: blnd_id.clone(),
        usdc_id: usdc_id.clone(),
    };
    pool_factory_client.initialize(&pool_init_meta);

    // verify initialize can't be run twice
    let result = pool_factory_client.try_initialize(&pool_init_meta);
    assert!(result.is_err());

    let name1 = Symbol::new(&e, "pool1");
    let name2 = Symbol::new(&e, "pool2");
    let salt = BytesN::<32>::random(&e);
    let deployed_pool_address_1 =
        pool_factory_client.deploy(&bombadil, &name1, &salt, &oracle, &backstop_rate);

    let event = vec![&e, e.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &e,
            (
                pool_factory_address.clone(),
                (Symbol::new(&e, "deploy"),).into_val(&e),
                deployed_pool_address_1.to_val()
            )
        ]
    );

    let salt = BytesN::<32>::random(&e);
    let deployed_pool_address_2 =
        pool_factory_client.deploy(&bombadil, &name2, &salt, &oracle, &backstop_rate);

    let zero_address = Address::from_contract_id(&BytesN::from_array(&e, &[0; 32]));
    e.as_contract(&deployed_pool_address_1, || {
        assert_eq!(
            e.storage()
                .persistent()
                .get::<_, Address>(&Symbol::new(&e, "Admin"))
                .unwrap(),
            bombadil.clone()
        );
        assert_eq!(
            e.storage()
                .persistent()
                .get::<_, Address>(&Symbol::new(&e, "Backstop"))
                .unwrap(),
            backstop_id.clone()
        );
        assert_eq!(
            e.storage()
                .persistent()
                .get::<_, lending_pool::PoolConfig>(&Symbol::new(&e, "PoolConfig"))
                .unwrap(),
            lending_pool::PoolConfig {
                oracle: oracle,
                bstop_rate: backstop_rate,
                status: 1
            }
        );
        assert_eq!(
            e.storage()
                .persistent()
                .get::<_, Address>(&Symbol::new(&e, "BLNDTkn"))
                .unwrap(),
            blnd_id.clone()
        );
        assert_eq!(
            e.storage()
                .persistent()
                .get::<_, Address>(&Symbol::new(&e, "USDCTkn"))
                .unwrap(),
            usdc_id.clone()
        );
    });
    assert_ne!(deployed_pool_address_1, zero_address);
    assert_ne!(deployed_pool_address_2, zero_address);
    assert_ne!(deployed_pool_address_1, deployed_pool_address_2);
    assert!(pool_factory_client.is_pool(&deployed_pool_address_1));
    assert!(pool_factory_client.is_pool(&deployed_pool_address_2));
    assert!(!pool_factory_client.is_pool(&zero_address));
}
