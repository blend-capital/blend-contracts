#![cfg(test)]
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    unwrap::UnwrapOptimized,
    vec, Address, Env, IntoVal, Symbol, Vec,
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
    e.mock_all_auths();
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

    let pool_address = Address::random(&e);

    let (backstop_token_id, backstop_token_client) = create_token(&e, &bombadil);
    let (blnd_token_id, blnd_token_client) = create_token(&e, &bombadil);
    let (emitter_id, emitter_client) = create_emitter(&e);
    let (pool_factory_id, pool_factory_client) = create_mock_pool_factory(&e);
    pool_factory_client.set_pool(&pool_address);

    // create backstop module
    let (backstop_address, backstop_client) = create_backstop_module(&e);
    backstop_client.initialize(&backstop_token_id, &blnd_token_id, &pool_factory_id);

    // setup emissions
    backstop_client.add_reward(&pool_address, &pool_address);
    assert_eq!(e.auths(), []);

    blnd_token_client.set_admin(&emitter_id);
    emitter_client.initialize(&backstop_address, &blnd_token_id);
    emitter_client.distribute();

    // mint tokens to user and approve backstop
    let deposit_amount: i128 = 10_0000000;
    backstop_token_client.mint(&samwise, &deposit_amount);
    backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX);

    // deposit into backstop module
    let shares_minted = backstop_client.deposit(&samwise, &pool_address, &deposit_amount);
    assert_eq!(
        e.auths()[0],
        (
            samwise.clone(),
            backstop_address.clone(),
            Symbol::new(&e, "deposit"),
            vec![
                &e,
                samwise.clone().to_raw(),
                pool_address.clone().to_raw(),
                deposit_amount.into_val(&e)
            ]
        )
    );

    assert_eq!(backstop_token_client.balance(&samwise), 0);
    assert_eq!(
        backstop_token_client.balance(&backstop_address),
        deposit_amount
    );
    assert_eq!(
        backstop_client.balance(&pool_address, &samwise),
        deposit_amount
    );
    assert_eq!(
        backstop_client.pool_balance(&pool_address),
        (deposit_amount, deposit_amount, 0)
    );
    assert_eq!(shares_minted, deposit_amount); // 1-to-1 on first deposit

    // start emissions
    backstop_client.distribute();
    assert_eq!(e.auths(), []);
    assert_eq!(
        blnd_token_client.balance(&backstop_address),
        7 * 24 * 60 * 60 * 1_0000000
    );

    // queue for withdraw (all)
    let _q4w = backstop_client.queue_withdrawal(&samwise, &pool_address, &shares_minted);
    assert_eq!(
        e.auths()[0],
        (
            samwise.clone(),
            backstop_address.clone(),
            Symbol::new(&e, "queue_withdrawal"),
            vec![
                &e,
                samwise.clone().to_raw(),
                pool_address.clone().to_raw(),
                shares_minted.into_val(&e)
            ]
        )
    );

    assert_eq!(backstop_token_client.balance(&samwise), 0);
    assert_eq!(
        backstop_token_client.balance(&backstop_address),
        deposit_amount
    );
    assert_eq!(
        backstop_client.balance(&pool_address, &samwise),
        deposit_amount
    );
    assert_eq!(
        backstop_client.pool_balance(&pool_address),
        (deposit_amount, deposit_amount, shares_minted)
    );
    let cur_q4w = backstop_client.withdrawal_queue(&pool_address, &samwise);
    assert_eq!(cur_q4w.len(), 1);
    let first_q4w = cur_q4w.first().unwrap_optimized().unwrap_optimized();
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
    let amount_returned = backstop_client.withdraw(&samwise, &pool_address, &shares_minted);
    assert_eq!(
        e.auths()[0],
        (
            samwise.clone(),
            backstop_address.clone(),
            Symbol::new(&e, "withdraw"),
            vec![
                &e,
                samwise.clone().to_raw(),
                pool_address.clone().to_raw(),
                shares_minted.into_val(&e)
            ]
        )
    );

    assert_eq!(backstop_token_client.balance(&samwise), deposit_amount);
    assert_eq!(backstop_token_client.balance(&backstop_address), 0);
    assert_eq!(backstop_client.balance(&pool_address, &samwise), 0);
    assert_eq!(backstop_client.pool_balance(&pool_address), (0, 0, 0));
    assert_eq!(amount_returned, deposit_amount);
    let cur_q4w = backstop_client.withdrawal_queue(&pool_address, &samwise);
    assert_eq!(cur_q4w.len(), 0);

    // claim emissions
    let to_claim_vec: Vec<Address> = vec![&e, pool_address.clone()];
    backstop_client.claim(&samwise, &to_claim_vec, &samwise);
    assert_eq!(
        e.auths()[0],
        (
            samwise.clone(),
            backstop_address.clone(),
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
    backstop_client.pool_claim(&pool_address, &samwise, &1_1234567);
    assert_eq!(
        e.auths()[0],
        (
            pool_address.clone(),
            backstop_address.clone(),
            Symbol::new(&e, "pool_claim"),
            vec![
                &e,
                pool_address.clone().to_raw(),
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
