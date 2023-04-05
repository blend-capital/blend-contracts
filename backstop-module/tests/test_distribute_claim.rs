#![cfg(test)]
use common::generate_contract_id;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, BytesN, Env,
};

mod common;
use crate::common::{create_backstop_module, create_mock_pool_factory, create_token_from_id};

#[test]
fn test_backstop_distribution_and_claim_happy_path() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop = Address::from_contract_id(&e, &backstop_addr);
    let token_addr = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_addr, &bombadil);
    backstop_client.initialize(&token_addr);

    let pool_1 = generate_contract_id(&e); // in reward zone
    let pool_1_id = Address::from_contract_id(&e, &pool_1);
    let pool_2 = generate_contract_id(&e); // in reward zone
    let pool_3 = generate_contract_id(&e); // out of reward zone

    let mock_pool_factory = create_mock_pool_factory(&e);
    mock_pool_factory.set_pool(&pool_1);
    mock_pool_factory.set_pool(&pool_2);
    mock_pool_factory.set_pool(&pool_3);

    token_client.mint(&bombadil, &samwise, &600_000_0000000);
    token_client.incr_allow(&samwise, &backstop, &i128::MAX);

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    backstop_client.deposit(&samwise, &pool_1, &300_000_0000000);
    backstop_client.deposit(&samwise, &pool_2, &200_000_0000000);
    backstop_client.deposit(&samwise, &pool_3, &100_000_0000000);

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

    // claim
    backstop_client.claim(&pool_1_id, &pool_1, &samwise, &50_000_0000000);

    assert_eq!(token_client.balance(&samwise), 50_000_0000000);

    // verify claim doesn't break with an invalid contract
    // TODO: See `claim` for issue
    // let result = backstop_client.try_claim(&Address::random(&e), &pool_1, &samwise, &50_000_0000000);

    // match result {
    //     Ok(_) => assert!(false),
    //     Err(error) => match error {
    //         Ok(p_error) => assert_eq!(p_error, BackstopError::NotPool),
    //         Err(_) => assert!(false),
    //     },
    // }
}
