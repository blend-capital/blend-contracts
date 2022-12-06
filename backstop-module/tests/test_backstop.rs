#![cfg(test)]
use common::generate_contract_id;
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{
    testutils::{Accounts, Ledger, LedgerInfo},
    BytesN, Env,
};

mod common;
use crate::common::{create_backstop_module, create_token_from_id};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_pool_happy_path() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    let pool_addr = generate_contract_id(&e);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop_id = Identifier::Contract(backstop_addr);
    let token_id = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_id, &bombadil_id);

    e.ledger().set(LedgerInfo {
        timestamp: 0,
        protocol_version: 1,
        sequence_number: 0,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    // mint tokens to user and approve backstop
    let deposit_amount: u64 = 10_0000000;
    token_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &samwise_id,
        &(deposit_amount as i128),
    );
    token_client.with_source_account(&samwise).approve(
        &Signature::Invoker,
        &0,
        &backstop_id,
        &(u64::MAX as i128),
    );

    // deposit into backstop module
    let shares_minted = backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_addr, &deposit_amount);

    assert_eq!(token_client.balance(&samwise_id), 0);
    assert_eq!(token_client.balance(&backstop_id), deposit_amount as i128);
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
    assert_eq!(token_client.balance(&backstop_id), deposit_amount as i128);
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
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    // withdraw
    let amount_returned = backstop_client
        .with_source_account(&samwise)
        .withdraw(&pool_addr, &shares_minted);

    assert_eq!(token_client.balance(&samwise_id), deposit_amount as i128);
    assert_eq!(token_client.balance(&backstop_id), 0);
    assert_eq!(backstop_client.balance(&pool_addr, &samwise_id), 0);
    assert_eq!(backstop_client.p_balance(&pool_addr), (0, 0, 0));
    assert_eq!(amount_returned, deposit_amount);
    let cur_q4w = backstop_client.q4w(&pool_addr, &samwise_id);
    assert_eq!(cur_q4w.len(), 0);
}
