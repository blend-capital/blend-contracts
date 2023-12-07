#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Events},
    vec, Address, BytesN, Env, IntoVal, Symbol,
};

use crate::{PoolFactoryClient, PoolFactoryContract, PoolInitMeta};

mod pool {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/pool.wasm");
}

fn create_pool_factory(e: &Env) -> (Address, PoolFactoryClient) {
    let contract_id = e.register_contract(None, PoolFactoryContract {});
    (contract_id.clone(), PoolFactoryClient::new(e, &contract_id))
}

#[test]
fn test_pool_factory() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths_allowing_non_root_auth();
    let (pool_factory_address, pool_factory_client) = create_pool_factory(&e);

    let wasm_hash = e.deployer().upload_contract_wasm(pool::WASM);

    let bombadil = Address::generate(&e);

    let oracle = Address::generate(&e);
    let backstop_id = Address::generate(&e);
    let backstop_rate: u64 = 100000;
    let blnd_id = Address::generate(&e);
    let usdc_id = Address::generate(&e);

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

    e.as_contract(&deployed_pool_address_1, || {
        assert_eq!(
            e.storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&e, "Admin"))
                .unwrap(),
            bombadil.clone()
        );
        assert_eq!(
            e.storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&e, "Backstop"))
                .unwrap(),
            backstop_id.clone()
        );
        assert_eq!(
            e.storage()
                .instance()
                .get::<_, pool::PoolConfig>(&Symbol::new(&e, "Config"))
                .unwrap(),
            pool::PoolConfig {
                oracle: oracle,
                bstop_rate: backstop_rate,
                status: 1
            }
        );
        assert_eq!(
            e.storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&e, "BLNDTkn"))
                .unwrap(),
            blnd_id.clone()
        );
        assert_eq!(
            e.storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&e, "USDCTkn"))
                .unwrap(),
            usdc_id.clone()
        );
    });
    assert_ne!(deployed_pool_address_1, deployed_pool_address_2);
    assert!(pool_factory_client.is_pool(&deployed_pool_address_1));
    assert!(pool_factory_client.is_pool(&deployed_pool_address_2));
    assert!(!pool_factory_client.is_pool(&Address::generate(&e)));
}
