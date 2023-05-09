#![cfg(test)]
use cast::i128;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    vec, Address, Env, IntoVal, Symbol, Vec,
};

mod common;
use crate::common::{
    create_mock_oracle, create_token, create_wasm_lending_pool, pool_helper, BackstopClient,
    BlendTokenClient, ReserveEmissionMetadata, TokenClient,
};

const START_TIME: u64 = 1441065600;

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_pool_wasm_smoke() {
    let e = Env::default();
    // disable limits for test
    e.budget().reset_unlimited();

    e.ledger().set(LedgerInfo {
        timestamp: START_TIME,
        protocol_version: 1,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);
    let samwise = Address::random(&e);

    let (oracle_id, mock_oracle_client) = create_mock_oracle(&e);
    let (blnd_id, blnd_token_client) = create_token(&e, &bombadil);
    let (usdc_id, _) = create_token(&e, &bombadil);

    let (pool_id, pool_client) = create_wasm_lending_pool(&e);
    let pool = Address::from_contract_id(&e, &pool_id);
    let name: Symbol = Symbol::new(&e, "pool1");
    let backstop_id = pool_helper::setup_pool(
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
    let backstop_client = BackstopClient::new(&e, &backstop_id);

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

    // setup emissions
    backstop_client.add_reward(&pool_id, &pool_id);
    backstop_client.dist();
    pool_client.set_emis(
        &bombadil,
        &vec![
            &e,
            ReserveEmissionMetadata {
                res_index: 0,
                res_type: 0,
                share: 1,
            },
        ],
    );
    pool_client.updt_emis();
    assert_eq!(e.recorded_top_authorizations(), []);

    // supply
    let minted_btokens = pool_client.supply(&samwise, &asset1_id, &supply_amount);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            pool_id.clone(),
            Symbol::new(&e, "supply"),
            vec![
                &e,
                samwise.clone().to_raw(),
                asset1_id.clone().to_raw(),
                supply_amount.into_val(&e)
            ]
        )
    );

    assert_eq!(asset1_client.balance(&samwise), 0);
    assert_eq!(asset1_client.balance(&pool), supply_amount);
    assert_eq!(b_token1_client.balance(&samwise), minted_btokens);
    assert_eq!(minted_btokens, 2_0000000);
    assert_eq!(pool_client.config(&samwise), 2);

    // borrow
    let borrow_amount = 1_0000000;
    let minted_dtokens = pool_client.borrow(&samwise, &asset1_id, &borrow_amount, &samwise);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            pool_id.clone(),
            Symbol::new(&e, "borrow"),
            vec![
                &e,
                samwise.clone().to_raw(),
                asset1_id.clone().to_raw(),
                borrow_amount.into_val(&e),
                samwise.clone().to_raw(),
            ]
        )
    );

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
        timestamp: START_TIME + 31536000, // 1 year
        protocol_version: 1,
        sequence_number: 6307200,
        network_id: Default::default(),
        base_reserve: 10,
    });

    // repay
    let burnt_dtokens = pool_client.repay(&samwise, &asset1_id, &borrow_amount, &samwise);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            pool_id.clone(),
            Symbol::new(&e, "repay"),
            vec![
                &e,
                samwise.clone().to_raw(),
                asset1_id.clone().to_raw(),
                borrow_amount.into_val(&e),
                samwise.clone().to_raw(),
            ]
        )
    );

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
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            pool_id.clone(),
            Symbol::new(&e, "withdraw"),
            vec![
                &e,
                samwise.clone().to_raw(),
                asset1_id.clone().to_raw(),
                (supply_amount + user_interest_accrued).into_val(&e),
                samwise.clone().to_raw(),
            ]
        )
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

    // claim
    let to_claim_vec: Vec<u32> = vec![&e, 0_u32];
    let claimed = pool_client.claim(&samwise, &to_claim_vec, &samwise);
    assert_eq!(
        e.recorded_top_authorizations()[0],
        (
            samwise.clone(),
            pool_id.clone(),
            Symbol::new(&e, "claim"),
            vec![
                &e,
                samwise.clone().to_raw(),
                to_claim_vec.into_val(&e),
                samwise.clone().to_raw()
            ]
        )
    );
    assert_eq!(blnd_token_client.balance(&samwise), claimed);
}
