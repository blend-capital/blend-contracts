#![cfg(test)]
use cast::i128;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, Env, Symbol,
};

mod common;
use crate::common::{
    create_mock_oracle, create_token, create_wasm_lending_pool, pool_helper, BlendTokenClient,
    TokenClient,
};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_pool_happy_path() {
    let e = Env::default();
    // disable limits for test
    e.budget().reset_unlimited();

    e.ledger().set(LedgerInfo {
        timestamp: 0,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);
    let (blnd_id, _) = create_token(&e, &bombadil);
    let (usdc_id, _) = create_token(&e, &bombadil);

    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let name: Symbol = Symbol::new(&e, "pool1");
    pool_helper::setup_pool(
        &e,
        &pool_id,
        &pool_client,
        &bombadil,
        &name,
        &oracle_id,
        0_200_000_000,
        &blnd_id,
        &usdc_id,
    );

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(
        &e,
        &pool_client,
        &bombadil,
        &pool_helper::default_reserve_metadata(),
    );
    let asset1_client = TokenClient::new(&e, &asset1_id);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let supply_amount: i128 = 2_0000000;
    asset1_client.mint(&bombadil, &samwise, &supply_amount);
    asset1_client.incr_allow(&samwise, &pool, &i128(u64::MAX));
    assert_eq!(asset1_client.balance(&samwise), supply_amount);

    // supply
    let minted_btokens = pool_client.supply(&samwise, &asset1_id, &supply_amount);

    assert_eq!(asset1_client.balance(&samwise), 0);
    assert_eq!(asset1_client.balance(&pool), supply_amount);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    assert_eq!(minted_btokens, 2_0000000);
    assert_eq!(pool_client.config(&samwise), 2);

    // borrow
    let borrow_amount = 1_0000000;
    let minted_dtokens = pool_client.borrow(&samwise, &asset1_id, &borrow_amount, &samwise);

    assert_eq!(asset1_client.balance(&samwise), borrow_amount);
    assert_eq!(
        asset1_client.balance(&pool),
        (supply_amount - borrow_amount)
    );
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    assert_eq!(d_token1_client.balance(&samwise), minted_dtokens);
    assert_eq!(minted_dtokens, 1_0000000);
    assert_eq!(pool_client.config(&samwise), 3);

    // allow interest to accumulate
    // IR -> 6%
    e.ledger().set(LedgerInfo {
        timestamp: 6307200 * 5,
        protocol_version: 1,
        sequence_number: 6307200, // 1 year
        network_id: Default::default(),
        base_reserve: 10,
    });

    // repay
    let burnt_dtokens = pool_client.repay(&samwise, &asset1_id, &borrow_amount, &samwise);

    assert_eq!(asset1_client.balance(&samwise), 0);
    assert_eq!(asset1_client.balance(&pool), supply_amount);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    assert_eq!(d_token1_client.balance(&samwise), 566038);
    assert_eq!(burnt_dtokens, minted_dtokens - 566038);
    assert_eq!(pool_client.config(&samwise), 3);

    // repay interest
    let interest_accrued = 0_0600001;
    asset1_client.mint(&bombadil, &samwise, &(interest_accrued));
    let burnt_dtokens_interest = pool_client.repay(&samwise, &asset1_id, &i128::MAX, &samwise);
    assert_eq!(asset1_client.balance(&samwise), 0);
    assert_eq!(
        asset1_client.balance(&pool),
        (supply_amount + interest_accrued)
    );
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    assert_eq!(d_token1_client.balance(&samwise), 0);
    assert_eq!(burnt_dtokens_interest, 566038);
    assert_eq!(pool_client.config(&samwise), 2);

    // withdraw
    let user_interest_accrued = 0_0477138;
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
    // the remaining funds due to the backstop based on the 20% backstop_rate, +1 because of rounding
    assert_eq!(asset1_client.balance(&pool), 122862 + 1);
    assert_eq!(b_token1_client.balance(&samwise), 0);
    assert_eq!(burnt_btokens, minted_btokens);
    assert_eq!(pool_client.config(&samwise), 0);
}
