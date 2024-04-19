#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Events},
    vec, Address, BytesN, Env, IntoVal, String, Symbol,
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
    let backstop_rate: u32 = 0_1000000;
    let max_positions: u32 = 6;
    let blnd_id = Address::generate(&e);

    let pool_init_meta = PoolInitMeta {
        backstop: backstop_id.clone(),
        pool_hash: wasm_hash.clone(),
        blnd_id: blnd_id.clone(),
    };
    pool_factory_client.initialize(&pool_init_meta);

    // verify initialize can't be run twice
    let result = pool_factory_client.try_initialize(&pool_init_meta);
    assert!(result.is_err());

    let name1 = String::from_str(&e, "pool1");
    let name2 = String::from_str(&e, "pool2");
    let salt = BytesN::<32>::random(&e);

    let deployed_pool_address_1 = pool_factory_client.deploy(
        &bombadil,
        &name1,
        &salt,
        &oracle,
        &backstop_rate,
        &max_positions,
    );

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
    let deployed_pool_address_2 = pool_factory_client.deploy(
        &bombadil,
        &name2,
        &salt,
        &oracle,
        &backstop_rate,
        &max_positions,
    );

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
                status: 6,
                max_positions: 6
            }
        );
        assert_eq!(
            e.storage()
                .instance()
                .get::<_, Address>(&Symbol::new(&e, "BLNDTkn"))
                .unwrap(),
            blnd_id.clone()
        );
    });
    assert_ne!(deployed_pool_address_1, deployed_pool_address_2);
    assert!(pool_factory_client.is_pool(&deployed_pool_address_1));
    assert!(pool_factory_client.is_pool(&deployed_pool_address_2));
    assert!(!pool_factory_client.is_pool(&Address::generate(&e)));
}

#[test]
#[should_panic(expected = "Error(Contract, #1300)")]
fn test_pool_factory_invalid_pool_init_args_backstop_rate() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths_allowing_non_root_auth();
    let (_, pool_factory_client) = create_pool_factory(&e);

    let wasm_hash = e.deployer().upload_contract_wasm(pool::WASM);

    let backstop_id = Address::generate(&e);
    let blnd_id = Address::generate(&e);

    let pool_init_meta = PoolInitMeta {
        backstop: backstop_id.clone(),
        pool_hash: wasm_hash.clone(),
        blnd_id: blnd_id.clone(),
    };
    pool_factory_client.initialize(&pool_init_meta);

    let bombadil = Address::generate(&e);
    let oracle = Address::generate(&e);
    let backstop_rate: u32 = 1_0000000;
    let max_positions: u32 = 6;

    let name1 = String::from_str(&e, "pool1");
    let salt = BytesN::<32>::random(&e);

    pool_factory_client.deploy(
        &bombadil,
        &name1,
        &salt,
        &oracle,
        &backstop_rate,
        &max_positions,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #1300)")]
fn test_pool_factory_invalid_pool_init_args_max_positions() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths_allowing_non_root_auth();
    let (_, pool_factory_client) = create_pool_factory(&e);

    let wasm_hash = e.deployer().upload_contract_wasm(pool::WASM);

    let backstop_id = Address::generate(&e);
    let blnd_id = Address::generate(&e);

    let pool_init_meta = PoolInitMeta {
        backstop: backstop_id.clone(),
        pool_hash: wasm_hash.clone(),
        blnd_id: blnd_id.clone(),
    };
    pool_factory_client.initialize(&pool_init_meta);

    let bombadil = Address::generate(&e);
    let oracle = Address::generate(&e);
    let backstop_rate: u32 = 0_1000000;
    let max_positions: u32 = 1;

    let name1 = String::from_str(&e, "pool1");
    let salt = BytesN::<32>::random(&e);

    pool_factory_client.deploy(
        &bombadil,
        &name1,
        &salt,
        &oracle,
        &backstop_rate,
        &max_positions,
    );
}

#[test]
fn test_pool_factory_frontrun_protection() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.mock_all_auths();

    let (_, pool_factory_client) = create_pool_factory(&e);

    let wasm_hash = e.deployer().upload_contract_wasm(pool::WASM);

    let bombadil = Address::generate(&e);
    let sauron = Address::generate(&e);

    let oracle = Address::generate(&e);
    let backstop_id = Address::generate(&e);
    let backstop_rate: u32 = 0_1000000;
    let max_positions: u32 = 6;
    let blnd_id = Address::generate(&e);

    let pool_init_meta = PoolInitMeta {
        backstop: backstop_id.clone(),
        pool_hash: wasm_hash.clone(),
        blnd_id: blnd_id.clone(),
    };
    pool_factory_client.initialize(&pool_init_meta);

    let name1 = String::from_str(&e, "pool1");
    let name2 = String::from_str(&e, "pool_front_run");
    let salt = BytesN::<32>::random(&e);

    // verify two different users don't get the same pool address with the same
    // salt parameter
    e.budget().reset_unlimited();
    let deployed_pool_address_sauron = pool_factory_client.deploy(
        &sauron,
        &name2,
        &salt,
        &oracle,
        &backstop_rate,
        &max_positions,
    );
    e.budget().print();

    let deployed_pool_address_bombadil = pool_factory_client.deploy(
        &bombadil,
        &name1,
        &salt,
        &oracle,
        &backstop_rate,
        &max_positions,
    );

    assert!(deployed_pool_address_sauron != deployed_pool_address_bombadil);
    assert!(pool_factory_client.is_pool(&deployed_pool_address_sauron));
    assert!(pool_factory_client.is_pool(&deployed_pool_address_bombadil));
}
