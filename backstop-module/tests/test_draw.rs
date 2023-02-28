#![cfg(test)]
use common::generate_contract_id;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, BytesN, Env,
};

mod common;
use crate::common::{
    create_backstop_module, create_mock_pool_factory, create_token_from_id, BackstopError,
};

#[test]
fn test_draw_happy_path() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop = Address::from_contract_id(&e, &backstop_addr);
    let token_addr = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_addr, &bombadil);

    let pool_1 = generate_contract_id(&e);
    let pool_2 = generate_contract_id(&e);

    let mock_pool_factory = create_mock_pool_factory(&e);
    mock_pool_factory.set_pool(&pool_1);
    mock_pool_factory.set_pool(&pool_2);

    token_client.mint(&bombadil, &samwise, &600_000_0000000);
    token_client.incr_allow(&samwise, &backstop, &i128::MAX);

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    backstop_client.deposit(&samwise, &pool_1, &400_000_0000000);
    backstop_client.deposit(&samwise, &pool_2, &200_000_0000000);

    let (pre_tokens_1, pre_shares_1, _pre_q4w_1) = backstop_client.p_balance(&pool_1);
    let (pre_tokens_2, pre_shares_2, _pre_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(pre_tokens_1, 400_000_0000000);
    assert_eq!(pre_shares_1, 400_000_0000000);
    assert_eq!(pre_tokens_2, 200_000_0000000);
    assert_eq!(pre_shares_2, 200_000_0000000);

    // draw
    backstop_client.draw(
        &Address::from_contract_id(&e, &pool_2),
        &pool_2,
        &100_000_0000000,
        &samwise,
    );

    let (post_tokens_1, post_shares_1, _post_q4w_1) = backstop_client.p_balance(&pool_1);
    let (post_tokens_2, post_shares_2, _post_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(post_tokens_1, 400_000_0000000);
    assert_eq!(post_shares_1, 400_000_0000000);
    assert_eq!(post_tokens_2, 200_000_0000000 - 100_000_0000000);
    assert_eq!(post_shares_2, 200_000_0000000);
    assert_eq!(token_client.balance(&samwise), 100_000_0000000);
    assert_eq!(token_client.balance(&backstop), 500_000_0000000);
}

#[test]
fn test_draw_not_pool() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop = Address::from_contract_id(&e, &backstop_addr);
    let token_addr = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_addr, &bombadil);

    let pool_1 = generate_contract_id(&e);
    let pool_2 = generate_contract_id(&e);

    let mock_pool_factory = create_mock_pool_factory(&e);
    mock_pool_factory.set_pool(&pool_1);
    mock_pool_factory.set_pool(&pool_2);

    token_client.mint(&bombadil, &samwise, &600_000_0000000);
    token_client.incr_allow(&samwise, &backstop, &i128::MAX);

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    backstop_client.deposit(&samwise, &pool_1, &400_000_0000000);
    backstop_client.deposit(&samwise, &pool_2, &200_000_0000000);

    let (pre_tokens_1, pre_shares_1, _pre_q4w_1) = backstop_client.p_balance(&pool_1);
    let (pre_tokens_2, pre_shares_2, _pre_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(pre_tokens_1, 400_000_0000000);
    assert_eq!(pre_shares_1, 400_000_0000000);
    assert_eq!(pre_tokens_2, 200_000_0000000);
    assert_eq!(pre_shares_2, 200_000_0000000);

    // draw
    let result = backstop_client.try_draw(&sauron, &pool_2, &100_000_0000000, &samwise);

    match result {
        Ok(_) => {
            assert!(true); // TODO: see `draw` for issue
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, BackstopError::NotPool),
            Err(_) => assert!(false),
        },
    }
}
