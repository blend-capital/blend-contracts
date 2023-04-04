#![cfg(test)]
use common::generate_contract_id;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, BytesN, Env,
};

mod common;
use crate::common::{create_backstop_module, create_token_from_id};

#[test]
fn test_donate_happy_path() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop = Address::from_contract_id(&e, &backstop_addr);
    let token_addr = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_addr, &bombadil);
    backstop_client.initialize(&token_addr);

    let pool_1 = generate_contract_id(&e);
    let pool_2 = generate_contract_id(&e);
    let pool_2_id = Address::from_contract_id(&e, &pool_2);

    token_client.mint(&bombadil, &samwise, &700_000_0000000);
    token_client.incr_allow(&samwise, &backstop, &i128::MAX);

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    // deposit into backstop module
    backstop_client.deposit(&samwise, &pool_1, &400_000_0000000);
    backstop_client.deposit(&samwise, &pool_2, &200_000_0000000);

    let (pre_tokens_1, pre_shares_1, _pre_q4w_1) = backstop_client.p_balance(&pool_1);
    let (pre_tokens_2, pre_shares_2, _pre_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(pre_tokens_1, 400_000_0000000);
    assert_eq!(pre_shares_1, 400_000_0000000);
    assert_eq!(pre_tokens_2, 200_000_0000000);
    assert_eq!(pre_shares_2, 200_000_0000000);

    // donate
    token_client.incr_allow(&samwise, &pool_2_id, &i128::MAX);

    backstop_client.donate(&samwise, &pool_2, &100_000_0000000);

    let (post_tokens_1, post_shares_1, _post_q4w_1) = backstop_client.p_balance(&pool_1);
    let (post_tokens_2, post_shares_2, _post_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(post_tokens_1, 400_000_0000000);
    assert_eq!(post_shares_1, 400_000_0000000);
    assert_eq!(post_tokens_2, 200_000_0000000 + 100_000_0000000);
    assert_eq!(post_shares_2, 200_000_0000000);
    assert_eq!(token_client.balance(&samwise), 0);
    assert_eq!(token_client.balance(&backstop), 700_000_0000000);
}
