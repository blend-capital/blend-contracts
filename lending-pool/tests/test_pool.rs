#![cfg(test)]
use cast::i128;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, Env,
};

mod common;
use crate::common::{
    create_mock_oracle, create_wasm_lending_pool, generate_contract_id, pool_helper, TokenClient,
};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_pool_happy_path() {
    let e = Env::default();
    e.budget().reset_unlimited();
    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

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

    let (asset1_id, btoken1_id, dtoken1_id) =
        pool_helper::setup_reserve(&e, &pool, &pool_client, &bombadil);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let btoken1_id_client = TokenClient::new(&e, &btoken1_id);
    let dtoken1_id_client = TokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let supply_amount: i128 = 2_0000000;
    asset1_client.mint(&bombadil, &samwise, &supply_amount);
    asset1_client.incr_allow(&samwise, &pool, &i128(u64::MAX));
    assert_eq!(asset1_client.balance(&samwise), supply_amount);

    // supply
    let minted_btokens = pool_client.supply(&samwise, &asset1_id, &supply_amount);

    assert_eq!(asset1_client.balance(&samwise), 0);
    assert_eq!(asset1_client.balance(&pool), supply_amount);
    assert_eq!(btoken1_id_client.balance(&samwise), minted_btokens);
    assert_eq!(minted_btokens, 2_0000000);
    assert_eq!(pool_client.config(&samwise), 2);
    println!("supply successful");

    // borrow
    let borrow_amount = 1_0000000;
    let minted_dtokens = pool_client.borrow(&samwise, &asset1_id, &borrow_amount, &samwise);

    assert_eq!(asset1_client.balance(&samwise), borrow_amount);
    assert_eq!(
        asset1_client.balance(&pool),
        (supply_amount - borrow_amount)
    );
    assert_eq!(btoken1_id_client.balance(&samwise), minted_btokens);
    assert_eq!(dtoken1_id_client.balance(&samwise), minted_dtokens);
    assert_eq!(minted_dtokens, 1_0000000);
    assert_eq!(pool_client.config(&samwise), 3);
    println!("borrow successful");

    // allow interest to accumulate
    // IR -> 6%
    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 6307200, // 1 year
        network_id: Default::default(),
        base_reserve: 10,
    });

    // repay
    let burnt_dtokens = pool_client.repay(&samwise, &asset1_id, &borrow_amount, &samwise);

    assert_eq!(asset1_client.balance(&samwise), 0);
    assert_eq!(asset1_client.balance(&pool), supply_amount);
    assert_eq!(btoken1_id_client.balance(&samwise), minted_btokens);
    assert_eq!(dtoken1_id_client.balance(&samwise), 566038);
    assert_eq!(burnt_dtokens, minted_dtokens - 566038);
    assert_eq!(pool_client.config(&samwise), 3);
    println!("repay successful");

    // repay interest
    let interest_accrued = 0_0600000;
    asset1_client.mint(&bombadil, &samwise, &(interest_accrued));
    let burnt_dtokens_interest = pool_client.repay(&samwise, &asset1_id, &i128::MAX, &samwise);

    assert_eq!(asset1_client.balance(&samwise), 0);
    assert_eq!(
        asset1_client.balance(&pool),
        (supply_amount + interest_accrued)
    );
    assert_eq!(btoken1_id_client.balance(&samwise), minted_btokens);
    assert_eq!(dtoken1_id_client.balance(&samwise), 0);
    assert_eq!(burnt_dtokens_interest, 566038);
    assert_eq!(pool_client.config(&samwise), 2);
    println!("full repay successful");

    // withdraw
    let user_interest_accrued = 0_0480000;
    let burnt_btokens = pool_client.withdraw(
        &samwise,
        &asset1_id,
        &(supply_amount + user_interest_accrued),
        &samwise,
    );

    assert_eq!(
        asset1_client.balance(&samwise),
        (supply_amount + user_interest_accrued)
    );
    // the remaining funds due to the backstop based on the 20% backstop_rate
    assert_eq!(asset1_client.balance(&pool), 120000);
    assert_eq!(btoken1_id_client.balance(&samwise), 0);
    assert_eq!(burnt_btokens, minted_btokens);
    assert_eq!(pool_client.config(&samwise), 0);
    println!("withdraw successful");
}
