#![cfg(test)]
use cast::i128;
use soroban_sdk::{
    contractimpl, contracttype, map,
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, BytesN, Env, Map, RawVal, Status,
};

mod common;
use crate::common::{
    create_mock_oracle, create_wasm_lending_pool, generate_contract_id, pool_helper, PoolError,
    TokenClient, BlendTokenClient
};

#[test]
fn test_pool_borrow_no_collateral_panics() {
    let e = Env::default();

    let bombadil = Address::random(&e);
    let sauron = Address::random(&e);

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil, &pool_helper::default_reserve_metadata());
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

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

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil, &pool_helper::default_reserve_metadata());
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);
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

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil, &pool_helper::default_reserve_metadata());
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);
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

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil, &pool_helper::default_reserve_metadata());
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);
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

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil, &pool_helper::default_reserve_metadata());
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);
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

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil, &pool_helper::default_reserve_metadata());
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);
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

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);

    let backstop_id = generate_contract_id(&e);
    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    pool_helper::setup_pool(&e, &pool_client, &bombadil, &oracle_id, &backstop_id, 0_200_000_000);

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil, &pool_helper::default_reserve_metadata());
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &1_0000000);
    asset1_client.mint(&bombadil, &samwise, &10_0000000);
    asset1_client.incr_allow(&samwise, &pool, &i128(u64::MAX));

    // supply
    let minted_btokens = pool_client.supply(&samwise, &asset1_id, &4);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    println!("supplied");
    // borrow
    let minted_dtokens = pool_client.borrow(&samwise, &asset1_id, &1, &samwise);
    assert_eq!(d_token1_client.balance(&samwise), minted_dtokens);
    println!("borrow");

    // allow interest to accumulate
    // IR -> 6%
    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 6307200 * 60, // 30 years
        network_id: Default::default(),
        base_reserve: 10,
    });

    // user now has insufficient collateral - attempt to liquidate

    let liq_data = common::LiquidationMetadata {
        collateral: map![&e, (asset1_id.clone(), 4)],
        liability: map![&e, (asset1_id, 3)],
    };
    let result = pool_client.try_new_liq_a(&samwise, &liq_data);
    let expected_data = common::AuctionData {
        lot: map![&e, (0, 2)],
        bid: map![&e, (0, 1)],
        block: 6307200 * 3,
    };
    match result {
        Ok(_) => assert_eq!(result.unwrap().unwrap(), expected_data),
        Err(_) => {
            println!("{:?}", (result.unwrap().unwrap()));
            assert!(false)
        }
    }
}
