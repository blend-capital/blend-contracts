#![cfg(test)]
use cast::i128;
use common::generate_contract_id;
use soroban_auth::{Address, Signature};
use soroban_sdk::{
    testutils::{Accounts, Ledger, LedgerInfo},
    BytesN, Env,
};

mod common;
use crate::common::{create_backstop_module, create_token_from_id};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_backstop_happy_path() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Address::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Address::Account(samwise.clone());

    let pool_addr = generate_contract_id(&e);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop_id = Address::Contract(backstop_addr);
    let token_id = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_id, &bombadil_id);

    e.ledger().set(LedgerInfo {
        timestamp: 0,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    // mint tokens to user and approve backstop
    let deposit_amount: i128 = 10_0000000;
    token_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &samwise_id,
        &i128(deposit_amount),
    );
    token_client.with_source_account(&samwise).incr_allow(
        &Signature::Invoker,
        &0,
        &backstop_id,
        &i128(i128::MAX),
    );

    // deposit into backstop module
    let shares_minted = backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_addr, &deposit_amount);

    assert_eq!(token_client.balance(&samwise_id), 0);
    assert_eq!(token_client.balance(&backstop_id), i128(deposit_amount));
    assert_eq!(
        backstop_client.balance(&pool_addr, &samwise_id),
        deposit_amount
    );
    assert_eq!(
        backstop_client.p_balance(&pool_addr),
        (deposit_amount, deposit_amount, 0)
    );
    assert_eq!(shares_minted, deposit_amount); // 1-to-1 on first deposit

    // queue for withdraw (all)
    let _q4w = backstop_client
        .with_source_account(&samwise)
        .q_withdraw(&pool_addr, &shares_minted);

    assert_eq!(token_client.balance(&samwise_id), 0);
    assert_eq!(token_client.balance(&backstop_id), i128(deposit_amount));
    assert_eq!(
        backstop_client.balance(&pool_addr, &samwise_id),
        deposit_amount
    );
    assert_eq!(
        backstop_client.p_balance(&pool_addr),
        (deposit_amount, deposit_amount, shares_minted)
    );
    let cur_q4w = backstop_client.q4w(&pool_addr, &samwise_id);
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
    let amount_returned = backstop_client
        .with_source_account(&samwise)
        .withdraw(&pool_addr, &shares_minted);

    assert_eq!(token_client.balance(&samwise_id), i128(deposit_amount));
    assert_eq!(token_client.balance(&backstop_id), 0);
    assert_eq!(backstop_client.balance(&pool_addr, &samwise_id), 0);
    assert_eq!(backstop_client.p_balance(&pool_addr), (0, 0, 0));
    assert_eq!(amount_returned, deposit_amount);
    let cur_q4w = backstop_client.q4w(&pool_addr, &samwise_id);
    assert_eq!(cur_q4w.len(), 0);
}

#[test]
fn test_backstop_distribution_happy_path() {
    let e = Env::default();

    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop_id = Address::Contract(backstop_addr.clone());

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Address::Account(bombadil.clone());

    let token_id = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_id, &bombadil_id);

    let pool_1 = generate_contract_id(&e); // in reward zone
    let pool_2 = generate_contract_id(&e); // in reward zone
    let pool_3 = generate_contract_id(&e); // out of reward zone

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Address::Account(samwise.clone());

    token_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &samwise_id,
        &600_000_0000000,
    );
    token_client.with_source_account(&samwise).incr_allow(
        &Signature::Invoker,
        &0,
        &backstop_id,
        &i128(i128::MAX),
    );

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_1, &300_000_0000000);
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_2, &200_000_0000000);
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_3, &100_000_0000000);

    let (pre_tokens_1, pre_shares_1, _pre_q4w_1) = backstop_client.p_balance(&pool_1);
    let (pre_tokens_2, pre_shares_2, _pre_q4w_2) = backstop_client.p_balance(&pool_2);
    let (pre_tokens_3, pre_shares_3, _pre_q4w_3) = backstop_client.p_balance(&pool_3);

    assert_eq!(pre_tokens_1, 300_000_0000000);
    assert_eq!(pre_shares_1, 300_000_0000000);
    assert_eq!(pre_tokens_2, 200_000_0000000);
    assert_eq!(pre_shares_2, 200_000_0000000);
    assert_eq!(pre_tokens_3, 100_000_0000000);
    assert_eq!(pre_shares_3, 100_000_0000000);

    backstop_client.add_reward(&pool_1, &BytesN::from_array(&e, &[0u8; 32]));
    backstop_client.add_reward(&pool_2, &BytesN::from_array(&e, &[0u8; 32]));

    // distribute
    backstop_client.dist();

    let (post_tokens_1, post_shares_1, _post_q4w_1) = backstop_client.p_balance(&pool_1);
    let (post_tokens_2, post_shares_2, _post_q4w_2) = backstop_client.p_balance(&pool_2);
    let (post_tokens_3, post_shares_3, _post_q4w_3) = backstop_client.p_balance(&pool_3);

    assert_eq!(post_tokens_1, 300_000_0000000 + 210_000_0000000);
    assert_eq!(post_shares_1, 300_000_0000000);
    assert_eq!(post_tokens_2, 200_000_0000000 + 140_000_0000000);
    assert_eq!(post_shares_2, 200_000_0000000);
    assert_eq!(post_tokens_3, 100_000_0000000);
    assert_eq!(post_shares_3, 100_000_0000000);

    assert_eq!(backstop_client.pool_eps(&pool_1), 0_1800000);
    assert_eq!(backstop_client.pool_eps(&pool_2), 0_1200000);
    assert_eq!(backstop_client.pool_eps(&pool_3), 0);
}
