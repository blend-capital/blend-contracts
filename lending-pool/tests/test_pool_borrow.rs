#![cfg(test)]
use cast::i128;
use soroban_sdk::{
    map,
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, Env, Symbol,
};

mod common;
use crate::common::{
    create_mock_oracle, create_token, create_wasm_lending_pool, pool_helper, BlendTokenClient,
    TokenClient,
};

//TODO: IDK if this test is appropriate here
#[test]
fn test_pool_borrow_one_stroop_insufficient_collateral_for_two() {
    let e = Env::default();
    e.mock_all_auths();

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);
    let frodo = Address::random(&e);

    let (oracle_address, mock_oracle_client) = create_mock_oracle(&e);

    let (blnd_address, _) = create_token(&e, &bombadil);
    let (usdc_address, _) = create_token(&e, &bombadil);

    let (pool_address, pool_client) = create_wasm_lending_pool(&e);
    let name: Symbol = Symbol::new(&e, "pool1");
    pool_helper::setup_pool(
        &e,
        &pool_address,
        &pool_client,
        &bombadil,
        &name,
        &oracle_address,
        0_200_000_000,
        &blnd_address,
        &usdc_address,
    );
    e.budget().reset_unlimited();

    let (asset1_address, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(
        &e,
        &pool_client,
        &bombadil,
        &pool_helper::default_reserve_metadata(),
    );
    let asset1_client = TokenClient::new(&e, &asset1_address);
    let b_token1_client = BlendTokenClient::new(&e, &btoken1_id);
    let d_token1_client = BlendTokenClient::new(&e, &dtoken1_id);

    mock_oracle_client.set_price(&asset1_address, &1_0000000);
    asset1_client.mint(&samwise, &10_0000000);
    asset1_client.increase_allowance(&samwise, &pool_address, &i128(u64::MAX));
    asset1_client.mint(&frodo, &10_0000000);
    asset1_client.increase_allowance(&frodo, &pool_address, &i128(u64::MAX));

    let (asset2_id, b_token2_id, _d_token2_id) = pool_helper::setup_reserve(
        &e,
        &pool_client,
        &bombadil,
        &pool_helper::default_reserve_metadata(),
    );

    mock_oracle_client.set_price(&asset2_id, &1_0000000);

    let asset2_client = TokenClient::new(&e, &asset2_id);
    let b_token2_client = TokenClient::new(&e, &b_token2_id);
    asset2_client.mint(&frodo, &10_0000000);
    asset2_client.increase_allowance(&frodo, &pool_address, &i128(u64::MAX));
    e.budget().reset_unlimited();

    // supply
    let minted_btokens = pool_client.supply(&samwise, &asset1_address, &4);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    let minted_btokens2 = pool_client.supply(&frodo, &asset2_id, &6);
    assert_eq!(b_token2_client.balance(&frodo), minted_btokens2);

    // borrow
    let minted_dtokens = pool_client.borrow(&frodo, &asset1_address, &1, &frodo);
    assert_eq!(d_token1_client.balance(&frodo), minted_dtokens);

    // allow interest to accumulate
    // IR -> 3.5%
    e.ledger().set(LedgerInfo {
        timestamp: 6307200 * 90 * 5,
        protocol_version: 1,
        sequence_number: 6307200 * 90, // 90 years
        network_id: Default::default(),
        base_reserve: 10,
    });
    // user now has insufficient collateral - attempt to liquidate

    let liq_data = common::LiquidationMetadata {
        collateral: map![&e, (asset2_id.clone(), 6)],
        liability: map![&e, (asset1_address, 1)],
    };
    let result = pool_client.try_new_liquidation_auction(&frodo, &liq_data);
    let expected_data = common::AuctionData {
        lot: map![&e, (1, 6)],
        bid: map![&e, (0, 1)],
        block: 6307200 * 90 + 1,
    };
    match result {
        Ok(_) => assert_eq!(result.unwrap_optimized().unwrap_optimized(), expected_data),
        Err(_) => {
            println!("{:?}", (result.unwrap_optimized().unwrap_optimized()));
            assert!(false)
        }
    }
}
