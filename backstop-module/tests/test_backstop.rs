#![cfg(test)]
use common::generate_contract_id;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, BytesN, Env,
};

mod common;
use crate::common::{create_backstop_module, create_token_from_id};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_backstop_happy_path() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    let pool_addr = generate_contract_id(&e);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop = Address::from_contract_id(&e, &backstop_addr);
    let token_addr = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_addr, &bombadil);

    e.ledger().set(LedgerInfo {
        timestamp: 0,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    // mint tokens to user and approve backstop
    let deposit_amount: i128 = 10_0000000;
    token_client.mint(&bombadil, &samwise, &deposit_amount);
    token_client.incr_allow(&samwise, &backstop, &i128::MAX);

    // deposit into backstop module
    let shares_minted = backstop_client.deposit(&samwise, &pool_addr, &deposit_amount);

    assert_eq!(token_client.balance(&samwise), 0);
    assert_eq!(token_client.balance(&backstop), deposit_amount);
    assert_eq!(
        backstop_client.balance(&pool_addr, &samwise),
        deposit_amount
    );
    assert_eq!(
        backstop_client.p_balance(&pool_addr),
        (deposit_amount, deposit_amount, 0)
    );
    assert_eq!(shares_minted, deposit_amount); // 1-to-1 on first deposit

    // queue for withdraw (all)
    let _q4w = backstop_client.q_withdraw(&samwise, &pool_addr, &shares_minted);

    assert_eq!(token_client.balance(&samwise), 0);
    assert_eq!(token_client.balance(&backstop), deposit_amount);
    assert_eq!(
        backstop_client.balance(&pool_addr, &samwise),
        deposit_amount
    );
    assert_eq!(
        backstop_client.p_balance(&pool_addr),
        (deposit_amount, deposit_amount, shares_minted)
    );
    let cur_q4w = backstop_client.q4w(&pool_addr, &samwise);
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
    let amount_returned = backstop_client.withdraw(&samwise, &pool_addr, &shares_minted);

    assert_eq!(token_client.balance(&samwise), deposit_amount);
    assert_eq!(token_client.balance(&backstop), 0);
    assert_eq!(backstop_client.balance(&pool_addr, &samwise), 0);
    assert_eq!(backstop_client.p_balance(&pool_addr), (0, 0, 0));
    assert_eq!(amount_returned, deposit_amount);
    let cur_q4w = backstop_client.q4w(&pool_addr, &samwise);
    assert_eq!(cur_q4w.len(), 0);
}
