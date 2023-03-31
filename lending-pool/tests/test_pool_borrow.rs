#![cfg(test)]
use cast::i128;
use soroban_sdk::{
    map,
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, Env, Status,
};

mod common;
use crate::common::{
    create_mock_oracle, create_wasm_lending_pool, generate_contract_id, pool_helper, PoolError,
    TokenClient,
};

#[test]
fn test_pool_borrow_no_collateral_panics() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let sauron = Address::random(&e);

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);

    pool_client.initialize(
        &bombadil,
        &mock_oracle,
        &backstop_id,
        &backstop,
        &0_200_000_000,
    );
    pool_client.set_status(&bombadil, &0);

    let (asset1_id, _, _) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    // borrow
    let borrow_amount = 0_0000001;
    let result = pool_client.try_borrow(&sauron, &asset1_id, &borrow_amount, &sauron);
    // TODO: The try_borrow is returning a different error than it should
    match result {
        Ok(_) => assert!(false),
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidHf),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(3)),
        },
    }
}

#[test]
fn test_pool_borrow_bad_hf_panics() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let sauron = Address::random(&e);

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_client.initialize(
        &bombadil,
        &mock_oracle,
        &backstop_id,
        &backstop,
        &0_200_000_000,
    );
    pool_client.set_status(&bombadil, &0);

    let (asset1_id, b_token1_id, _) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = TokenClient::new(&e, &b_token1_id);
    asset1_client.mint(&bombadil, &sauron, &10_0000000);
    asset1_client.incr_allow(&sauron, &pool, &(u64::MAX as i128));

    let minted_btokens = pool_client.supply(&sauron, &asset1_id, &1_0000000);
    assert_eq!(b_token1_client.balance(&sauron), minted_btokens as i128);

    // borrow
    let borrow_amount = 0_5358000; // 0.75 cf * 0.75 lf => 0.5625 / 1.05 hf min => 0.5357 max
    let result = pool_client.try_borrow(&sauron, &asset1_id, &borrow_amount, &sauron);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidHf),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(3)),
        },
    }
}

#[test]
fn test_pool_borrow_good_hf_borrows() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_client.initialize(
        &bombadil,
        &mock_oracle,
        &backstop_id,
        &backstop,
        &0_200_000_000,
    );
    pool_client.set_status(&bombadil, &0);

    let (asset1_id, b_token1_id, d_token1_id) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = TokenClient::new(&e, &b_token1_id);
    let d_token1_client = TokenClient::new(&e, &d_token1_id);
    asset1_client.mint(&bombadil, &samwise, &10_0000000);
    asset1_client.incr_allow(&samwise, &pool, &(u64::MAX as i128));

    let minted_btokens = pool_client.supply(&samwise, &asset1_id, &1_0000000);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens as i128);

    // borrow
    let borrow_amount = 0_5357000; // 0.75 cf * 0.75 lf => 0.5625 / 1.05 hf min => 0.5357 max
    let minted_dtokens = pool_client.borrow(&samwise, &asset1_id, &borrow_amount, &samwise);
    assert_eq!(
        asset1_client.balance(&samwise),
        10_0000000 - 1_0000000 + 0_5357000
    );
    assert_eq!(asset1_client.balance(&pool), 1_0000000 - 0_5357000);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens as i128);
    assert_eq!(d_token1_client.balance(&samwise), minted_dtokens as i128);
}

#[test]
fn test_pool_borrow_on_ice_panics() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let sauron = Address::random(&e);

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_client.initialize(
        &bombadil,
        &mock_oracle,
        &backstop_id,
        &backstop,
        &0_200_000_000,
    );
    pool_client.set_status(&bombadil, &1);

    let (asset1_id, b_token1_id, _) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = TokenClient::new(&e, &b_token1_id);
    asset1_client.mint(&bombadil, &sauron, &10_0000000);
    asset1_client.incr_allow(&sauron, &pool, &(u64::MAX as i128));

    let minted_btokens = pool_client.supply(&sauron, &asset1_id, &1_0000000);
    assert_eq!(b_token1_client.balance(&sauron), minted_btokens as i128);

    // borrow
    let borrow_amount = 0_5358000; // 0.75 cf * 0.75 lf => 0.5625 / 1.05 hf min => 0.5357 max
    let result = pool_client.try_borrow(&sauron, &asset1_id, &borrow_amount, &sauron);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidPoolStatus),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(4)),
        },
    }
}

#[test]
fn test_pool_borrow_frozen_panics() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let sauron = Address::random(&e);

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_client.initialize(
        &bombadil,
        &mock_oracle,
        &backstop_id,
        &backstop,
        &0_200_000_000,
    );
    pool_client.set_status(&bombadil, &1);

    let (asset1_id, b_token1_id, _) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = TokenClient::new(&e, &b_token1_id);
    asset1_client.mint(&bombadil, &sauron, &10_0000000);
    asset1_client.incr_allow(&sauron, &pool, &(u64::MAX as i128));

    let minted_btokens = pool_client.supply(&sauron, &asset1_id, &1_0000000);
    assert_eq!(b_token1_client.balance(&sauron), minted_btokens as i128);

    pool_client.set_status(&bombadil, &2);

    // borrow
    let borrow_amount = 0_5358000; // 0.75 cf * 0.75 lf => 0.5625 / 1.05 hf min => 0.5357 max
    let result = pool_client.try_borrow(&sauron, &asset1_id, &borrow_amount, &sauron);
    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, PoolError::InvalidPoolStatus),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(4)),
        },
    }
}

// TODO: Unit test for issues/2
// -> d_rate > 1, try and borrow one stroop
#[test]
fn test_pool_borrow_one_stroop() {
    let e = Env::default();

    let bombadil = Address::random(&e);

    let samwise = Address::random(&e);

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_client.initialize(
        &bombadil,
        &mock_oracle,
        &backstop_id,
        &backstop,
        &0_200_000_000,
    );
    pool_client.set_status(&bombadil, &0);

    let (asset1_id, b_token1_id, d_token1_id) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = TokenClient::new(&e, &b_token1_id);
    let d_token1_client = TokenClient::new(&e, &d_token1_id);
    asset1_client.mint(&bombadil, &samwise, &10_0000000);
    asset1_client.incr_allow(&samwise, &pool, &i128(u64::MAX));

    // supply
    let minted_btokens = pool_client.supply(&samwise, &asset1_id, &2_0000000);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);

    // borrow
    let minted_dtokens = pool_client.borrow(&samwise, &asset1_id, &0_5355000, &samwise);
    assert_eq!(d_token1_client.balance(&samwise), minted_dtokens);

    // allow interest to accumulate
    // IR -> 6%
    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 6307200, // 1 year
        network_id: Default::default(),
        base_reserve: 10,
    });

    // borrow 1 stroop
    let borrow_amount = 0_0000001;
    let minted_dtokens_2 = pool_client.borrow(&samwise, &asset1_id, &borrow_amount, &samwise);
    assert_eq!(
        asset1_client.balance(&samwise),
        10_0000000 - 2_0000000 + 0_5355000 + 0_0000001
    );
    assert_eq!(
        asset1_client.balance(&pool),
        2_0000000 - 0_5355000 - 0_0000001
    );
    assert_eq!(
        d_token1_client.balance(&samwise),
        (minted_dtokens + minted_dtokens_2)
    );
    assert_eq!(minted_dtokens_2, 1);
}

//TODO: IDK if this test is appropriate here
#[test]
fn test_pool_borrow_one_stroop_insufficient_collateral_for_two() {
    let e = Env::default();

    let bombadil = Address::random(&e);

    let samwise = Address::random(&e);
    let frodo = Address::random(&e);

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_client.initialize(
        &bombadil,
        &mock_oracle,
        &backstop_id,
        &backstop,
        &0_200_000_000,
    );
    pool_client.set_status(&bombadil, &0);

    let (asset1_id, b_token1_id, d_token1_id) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset1_id, &1_0000000);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = TokenClient::new(&e, &b_token1_id);
    let d_token1_client = TokenClient::new(&e, &d_token1_id);
    asset1_client.mint(&bombadil, &samwise, &10_0000000);
    asset1_client.incr_allow(&samwise, &pool, &i128(u64::MAX));
    asset1_client.mint(&bombadil, &frodo, &10_0000000);
    asset1_client.incr_allow(&frodo, &pool, &i128(u64::MAX));

    let (asset2_id, b_token2_id, _d_token2_id) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    mock_oracle_client.set_price(&asset2_id, &1_0000000);

    let asset2_client = TokenClient::new(&e, &asset2_id);
    let b_token2_client = TokenClient::new(&e, &b_token2_id);
    asset2_client.mint(&bombadil, &frodo, &10_0000000);
    asset2_client.incr_allow(&frodo, &pool, &i128(u64::MAX));
    e.budget().reset();

    // supply
    let minted_btokens = pool_client.supply(&samwise, &asset1_id, &4);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    let minted_btokens2 = pool_client.supply(&frodo, &asset2_id, &6);
    assert_eq!(b_token2_client.balance(&frodo), minted_btokens2);

    // borrow
    let minted_dtokens = pool_client.borrow(&frodo, &asset1_id, &1, &frodo);
    assert_eq!(d_token1_client.balance(&frodo), minted_dtokens);

    // allow interest to accumulate
    // IR -> 3.5%
    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 6307200 * 90, // 90 years
        network_id: Default::default(),
        base_reserve: 10,
    });
    // user now has insufficient collateral - attempt to liquidate

    let liq_data = common::LiquidationMetadata {
        collateral: map![&e, (asset2_id.clone(), 6)],
        liability: map![&e, (asset1_id, 5)],
    };
    let result = pool_client.try_new_liq_a(&frodo, &liq_data);
    let expected_data = common::AuctionData {
        lot: map![&e, (1, 6)],
        bid: map![&e, (0, 1)],
        block: 6307200 * 90 + 1,
    };
    match result {
        Ok(_) => assert_eq!(result.unwrap().unwrap(), expected_data),
        Err(_) => {
            println!("{:?}", (result.unwrap().unwrap()));
            assert!(false)
        }
    }
}
