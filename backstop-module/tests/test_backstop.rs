#![cfg(test)]
use common::generate_contract_id;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, Env,
};

mod common;
use crate::common::{create_backstop_module, create_mock_pool_factory, create_token};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_backstop_happy_path() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    let pool_id = generate_contract_id(&e);

    let (backstop_token_id, backstop_token_client) = create_token(&e, &bombadil);
    let (blnd_token_id, _) = create_token(&e, &bombadil);
    let (pool_factory_id, pool_factory_client) = create_mock_pool_factory(&e);
    pool_factory_client.set_pool(&pool_id);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop = Address::from_contract_id(&e, &backstop_addr);
    backstop_client.initialize(&backstop_token_id, &blnd_token_id, &pool_factory_id);

    e.ledger().set(LedgerInfo {
        timestamp: 0,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    // mint tokens to user and approve backstop
    let deposit_amount: i128 = 10_0000000;
    backstop_token_client.mint(&bombadil, &samwise, &deposit_amount);
    backstop_token_client.incr_allow(&samwise, &backstop, &i128::MAX);

    // deposit into backstop module
    let shares_minted = backstop_client.deposit(&samwise, &pool_id, &deposit_amount);

    assert_eq!(backstop_token_client.balance(&samwise), 0);
    assert_eq!(backstop_token_client.balance(&backstop), deposit_amount);
    assert_eq!(backstop_client.balance(&pool_id, &samwise), deposit_amount);
    assert_eq!(
        backstop_client.p_balance(&pool_id),
        (deposit_amount, deposit_amount, 0)
    );
    assert_eq!(shares_minted, deposit_amount); // 1-to-1 on first deposit

    // queue for withdraw (all)
    let _q4w = backstop_client.q_withdraw(&samwise, &pool_id, &shares_minted);

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
    assert_eq!(first_q4w.exp, 30 * 24 * 60 * 60);

    // advance ledger 30 days
    e.ledger().set(LedgerInfo {
        timestamp: 30 * 24 * 60 * 60,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    // withdraw
    let amount_returned = backstop_client.withdraw(&samwise, &pool_id, &shares_minted);

    assert_eq!(backstop_token_client.balance(&samwise), deposit_amount);
    assert_eq!(backstop_token_client.balance(&backstop), 0);
    assert_eq!(backstop_client.balance(&pool_id, &samwise), 0);
    assert_eq!(backstop_client.p_balance(&pool_id), (0, 0, 0));
    assert_eq!(amount_returned, deposit_amount);
    let cur_q4w = backstop_client.q4w(&pool_id, &samwise);
    assert_eq!(cur_q4w.len(), 0);
}
