#![cfg(test)]
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{
    testutils::{Accounts, Ledger, LedgerInfo},
    Env,
};

mod common;
use crate::common::{create_mock_oracle, create_wasm_lending_pool, pool_helper, TokenClient};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_pool_happy_path() {
    let e = Env::default();

    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 0,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let user1 = e.accounts().generate_and_create();
    let user1_id = Identifier::Account(user1.clone());

    let (mock_oracle, mock_oracle_client) = create_mock_oracle(&e);

    let (pool, pool_client) = create_wasm_lending_pool(&e);
    let pool_id = Identifier::Contract(pool.clone());
    pool_client.initialize(&bombadil_id, &mock_oracle);
    pool_client.with_source_account(&bombadil).set_status(&0);

    let (asset1_id, btoken1_id, dtoken1_id) =
        pool_helper::setup_reserve(&e, &pool_id, &pool_client, &bombadil);

    let asset1_client = TokenClient::new(&e, &asset1_id);
    let btoken1_id_client = TokenClient::new(&e, &btoken1_id);
    let dtoken1_id_client = TokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_id, &2_0000000);

    let supply_amount = 2_0000000u64;
    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &user1_id,
        &(supply_amount as i128),
    );
    asset1_client.with_source_account(&user1).incr_allow(
        &Signature::Invoker,
        &0,
        &pool_id,
        &(u64::MAX as i128),
    );
    assert_eq!(asset1_client.balance(&user1_id), supply_amount as i128);

    // supply
    let minted_btokens = pool_client
        .with_source_account(&user1)
        .supply(&asset1_id, &supply_amount);

    assert_eq!(asset1_client.balance(&user1_id), 0);
    assert_eq!(asset1_client.balance(&pool_id), supply_amount as i128);
    assert_eq!(btoken1_id_client.balance(&user1_id), minted_btokens as i128);
    assert_eq!(minted_btokens, 2_0000000);
    assert_eq!(pool_client.config(&user1_id), 2);
    println!("supply successful");

    // borrow
    let borrow_amount = 1_0000000;
    let minted_dtokens =
        pool_client
            .with_source_account(&user1)
            .borrow(&asset1_id, &borrow_amount, &user1_id);

    assert_eq!(asset1_client.balance(&user1_id), borrow_amount as i128);
    assert_eq!(
        asset1_client.balance(&pool_id),
        (supply_amount - borrow_amount) as i128
    );
    assert_eq!(btoken1_id_client.balance(&user1_id), minted_btokens as i128);
    assert_eq!(dtoken1_id_client.balance(&user1_id), minted_dtokens as i128);
    assert_eq!(minted_dtokens, 1_0000000);
    assert_eq!(pool_client.config(&user1_id), 3);
    println!("borrow successful");

    // allow interest to accumulate
    // IR -> 6%
    e.ledger().set(LedgerInfo {
        timestamp: 12345,
        protocol_version: 1,
        sequence_number: 6307200, // 1 year
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    // repay
    let burnt_dtokens =
        pool_client
            .with_source_account(&user1)
            .repay(&asset1_id, &borrow_amount, &user1_id);

    assert_eq!(asset1_client.balance(&user1_id), 0);
    assert_eq!(asset1_client.balance(&pool_id), supply_amount as i128);
    assert_eq!(btoken1_id_client.balance(&user1_id), minted_btokens as i128);
    assert_eq!(dtoken1_id_client.balance(&user1_id), 566038);
    assert_eq!(burnt_dtokens, minted_dtokens - 566038);
    assert_eq!(pool_client.config(&user1_id), 3);
    println!("repay successful");

    // repay interest
    let interest_accrued = 0_0600000;
    asset1_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &user1_id,
        &(interest_accrued as i128),
    );
    let burnt_dtokens_interest =
        pool_client
            .with_source_account(&user1)
            .repay(&asset1_id, &u64::MAX, &user1_id);

    assert_eq!(asset1_client.balance(&user1_id), 0);
    assert_eq!(
        asset1_client.balance(&pool_id),
        (supply_amount + interest_accrued) as i128
    );
    assert_eq!(btoken1_id_client.balance(&user1_id), minted_btokens as i128);
    assert_eq!(dtoken1_id_client.balance(&user1_id), 0);
    assert_eq!(burnt_dtokens_interest, 566038);
    assert_eq!(pool_client.config(&user1_id), 2);
    println!("full repay successful");

    // withdraw
    let burnt_btokens = pool_client.with_source_account(&user1).withdraw(
        &asset1_id,
        &(supply_amount + interest_accrued),
        &user1_id,
    );

    assert_eq!(
        asset1_client.balance(&user1_id),
        (supply_amount + interest_accrued) as i128
    );
    assert_eq!(asset1_client.balance(&pool_id), 0);
    assert_eq!(btoken1_id_client.balance(&user1_id), 0);
    assert_eq!(burnt_btokens, minted_btokens);
    assert_eq!(pool_client.config(&user1_id), 0);
    println!("withdraw successful");
}
