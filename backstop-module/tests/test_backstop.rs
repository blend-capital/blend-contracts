#![cfg(test)]
use common::generate_contract_id;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    vec, Address, BytesN, Env, IntoVal, Symbol, Vec,
};

mod common;
use crate::common::{
    create_backstop_module, create_emitter, create_mock_pool_factory, create_token,
};

const START_TIME: u64 = 1441065600;

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_backstop_wasm_smoke() {
    let e = Env::default();
    e.budget().reset_unlimited();

    e.ledger().set(LedgerInfo {
        timestamp: START_TIME,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    let pool_id = generate_contract_id(&e);
    let pool_addr = Address::from_contract_id(&e, &pool_id);

    let (backstop_token_id, backstop_token_client) = create_token(&e, &bombadil);
    let (blnd_token_id, blnd_token_client) = create_token(&e, &bombadil);
    let (emitter_id, emitter_client) = create_emitter(&e);
    let (pool_factory_id, pool_factory_client) = create_mock_pool_factory(&e);
    pool_factory_client.set_pool(&pool_id);

    // create backstop module
    let (backstop_id, backstop_client) = create_backstop_module(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    backstop_client.initialize(&backstop_token_id, &blnd_token_id, &pool_factory_id);

    // setup emissions
    backstop_client.add_reward(&pool_id, &pool_id);
    assert_eq!(e.recorded_top_authorizations(), []);

    blnd_token_client.set_admin(&bombadil, &Address::from_contract_id(&e, &emitter_id));
    emitter_client.initialize(&backstop_id, &blnd_token_id);
    emitter_client.distribute();

    // mint tokens to user and approve backstop
    let deposit_amount: i128 = 10_0000000;
    backstop_token_client.mint(&bombadil, &samwise, &deposit_amount);
    backstop_token_client.incr_allow(&samwise, &backstop, &i128::MAX);

    // deposit into backstop module
    let shares_minted = backstop_client.deposit(&samwise, &pool_id, &deposit_amount);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            backstop_id.clone(),
            Symbol::new(&e, "deposit"),
            vec![
                &e,
                samwise.clone().to_raw(),
                pool_id.clone().to_raw(),
                deposit_amount.into_val(&e)
            ]
        )
    );

    assert_eq!(backstop_token_client.balance(&samwise), 0);
    assert_eq!(backstop_token_client.balance(&backstop), deposit_amount);
    assert_eq!(backstop_client.balance(&pool_id, &samwise), deposit_amount);
    assert_eq!(
        backstop_client.p_balance(&pool_id),
        (deposit_amount, deposit_amount, 0)
    );
    assert_eq!(shares_minted, deposit_amount); // 1-to-1 on first deposit

    // start emissions
    backstop_client.dist();
    assert_eq!(e.recorded_top_authorizations(), []);
    assert_eq!(
        blnd_token_client.balance(&backstop),
        7 * 24 * 60 * 60 * 1_0000000
    );

    // queue for withdraw (all)
    let _q4w = backstop_client.q_withdraw(&samwise, &pool_id, &shares_minted);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            backstop_id.clone(),
            Symbol::new(&e, "q_withdraw"),
            vec![
                &e,
                samwise.clone().to_raw(),
                pool_id.clone().to_raw(),
                shares_minted.into_val(&e)
            ]
        )
    );

    assert_eq!(backstop_token_client.balance(&samwise), 0);
    assert_eq!(backstop_token_client.balance(&backstop), deposit_amount);
    assert_eq!(backstop_client.balance(&pool_id, &samwise), deposit_amount);
    assert_eq!(
        backstop_client.p_balance(&pool_id),
        (deposit_amount, deposit_amount, shares_minted)
    );
    let cur_q4w = backstop_client.q4w(&pool_id, &samwise);
    assert_eq!(cur_q4w.len(), 1);
    let first_q4w = cur_q4w.first().unwrap().unwrap();
    assert_eq!(first_q4w.amount, shares_minted);
    assert_eq!(first_q4w.exp, START_TIME + 30 * 24 * 60 * 60);

    // advance ledger 30 days
    e.ledger().set(LedgerInfo {
        timestamp: START_TIME + 30 * 24 * 60 * 60,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    // withdraw
    let amount_returned = backstop_client.withdraw(&samwise, &pool_id, &shares_minted);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            backstop_id.clone(),
            Symbol::new(&e, "withdraw"),
            vec![
                &e,
                samwise.clone().to_raw(),
                pool_id.clone().to_raw(),
                shares_minted.into_val(&e)
            ]
        )
    );

    assert_eq!(backstop_token_client.balance(&samwise), deposit_amount);
    assert_eq!(backstop_token_client.balance(&backstop), 0);
    assert_eq!(backstop_client.balance(&pool_id, &samwise), 0);
    assert_eq!(backstop_client.p_balance(&pool_id), (0, 0, 0));
    assert_eq!(amount_returned, deposit_amount);
    let cur_q4w = backstop_client.q4w(&pool_id, &samwise);
    assert_eq!(cur_q4w.len(), 0);

    // claim emissions
    let to_claim_vec: Vec<BytesN<32>> = vec![&e, pool_id.clone()];
    backstop_client.claim(&samwise, &to_claim_vec, &samwise);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            backstop_id.clone(),
            Symbol::new(&e, "claim"),
            vec![
                &e,
                samwise.clone().to_raw(),
                to_claim_vec.to_raw(),
                samwise.clone().to_raw()
            ]
        )
    );
    assert_eq!(blnd_token_client.balance(&samwise), 423360 * 1_0000000);

    // pool claim emissions
    backstop_client.pool_claim(&pool_id, &samwise, &1_1234567);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            pool_addr.clone(),
            backstop_id.clone(),
            Symbol::new(&e, "pool_claim"),
            vec![
                &e,
                pool_id.clone().to_raw(),
                samwise.clone().to_raw(),
                1_1234567_i128.into_val(&e)
            ]
        )
    );
    assert_eq!(
        blnd_token_client.balance(&samwise),
        423360 * 1_0000000 + 1_1234567
    );
}
