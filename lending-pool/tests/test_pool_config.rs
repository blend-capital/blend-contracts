#![cfg(test)]
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    vec, Address, Env, IntoVal, Symbol,
};

mod common;
use crate::common::{
    create_mock_oracle, create_token, create_wasm_lending_pool, pool_helper,
    ReserveEmissionMetadata,
};

#[test]
fn test_pool_config() {
    let e = Env::default();
    // disable limits for test
    e.budget().reset_unlimited();

    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);

    let (oracle_id, _) = create_mock_oracle(&e);
    let (blnd_id, _) = create_token(&e, &bombadil);
    let (usdc_id, _) = create_token(&e, &bombadil);

    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let name: Symbol = Symbol::new(&e, "pool1");
    pool_helper::setup_pool(
        &e,
        &pool_id,
        &pool_client,
        &bombadil,
        &name,
        &oracle_id,
        0_200_000_000,
        &blnd_id,
        &usdc_id,
    );

    // validate admin config functions
    let mut reserve_meta = pool_helper::default_reserve_metadata();
    reserve_meta.l_factor = 0_7500000;
    pool_client.init_res(&bombadil, &usdc_id, &reserve_meta);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            bombadil.clone(),
            pool_id.clone(),
            Symbol::new(&e, "init_res"),
            vec![
                &e,
                bombadil.clone().to_raw(),
                usdc_id.clone().to_raw(),
                reserve_meta.clone().into_val(&e)
            ]
        )
    );
    assert_eq!(pool_client.res_config(&usdc_id).l_factor, 0_7500000);

    reserve_meta.l_factor = 0_9000000;
    pool_client.updt_res(&bombadil, &usdc_id, &reserve_meta);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            bombadil.clone(),
            pool_id.clone(),
            Symbol::new(&e, "updt_res"),
            vec![
                &e,
                bombadil.clone().to_raw(),
                usdc_id.clone().to_raw(),
                reserve_meta.clone().into_val(&e)
            ]
        )
    );
    assert_eq!(pool_client.res_config(&usdc_id).l_factor, 0_9000000);

    let emis_vec = vec![
        &e,
        ReserveEmissionMetadata {
            res_index: 0,
            res_type: 0,
            share: 1,
        },
    ];
    pool_client.set_emis(&bombadil, &emis_vec);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            bombadil.clone(),
            pool_id.clone(),
            Symbol::new(&e, "set_emis"),
            vec![&e, bombadil.clone().to_raw(), emis_vec.into_val(&e)]
        )
    );

    pool_client.set_status(&bombadil, &1);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            bombadil.clone(),
            pool_id.clone(),
            Symbol::new(&e, "set_status"),
            vec![&e, bombadil.clone().to_raw(), 1_u32.into_val(&e)]
        )
    );
    assert_eq!(pool_client.status(), 1);

    // validate user config functions
    let status_result = pool_client.updt_stat();
    assert_eq!(e.recorded_top_authorizations(), []);
    assert_eq!(pool_client.status(), 0);
    assert_eq!(status_result, 0);
}
