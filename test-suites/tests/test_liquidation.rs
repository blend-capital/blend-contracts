#![cfg(test)]
use fixed_point_math::FixedPoint;
use lending_pool::{PoolDataKey, Request, ReserveConfig, ReserveData};
use rand::distributions::FisherF;
use soroban_sdk::{
    map, storage, testutils::Address as AddressTestTrait, vec, Address, Symbol, Vec,
};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7},
};

#[test]
fn test_liquidations() {
    let (fixture, frodo) = create_fixture_with_data(false);
    let pool_fixture = &fixture.pools[0];

    // Disable rate modifiers
    let mut usdc_config: ReserveConfig = pool_fixture
        .pool
        .get_reserve_config(&fixture.tokens[TokenIndex::USDC].address);
    usdc_config.reactivity = 0;
    pool_fixture
        .pool
        .update_reserve(&fixture.tokens[TokenIndex::USDC].address, &usdc_config);
    let mut xlm_config: ReserveConfig = pool_fixture
        .pool
        .get_reserve_config(&fixture.tokens[TokenIndex::XLM].address);
    xlm_config.reactivity = 0;
    pool_fixture
        .pool
        .update_reserve(&fixture.tokens[TokenIndex::XLM].address, &xlm_config);
    let mut weth_config: ReserveConfig = pool_fixture
        .pool
        .get_reserve_config(&fixture.tokens[TokenIndex::WETH].address);
    weth_config.reactivity = 0;
    pool_fixture
        .pool
        .update_reserve(&fixture.tokens[TokenIndex::WETH].address, &weth_config);

    // Create a user
    let samwise = Address::random(&fixture.env); //sam will be supplying XLM and borrowing USDC

    // Mint users tokens
    fixture.tokens[TokenIndex::XLM].mint(&samwise, &(500_000 * SCALAR_7));
    fixture.tokens[TokenIndex::WETH].mint(&samwise, &(50 * 10i128.pow(9)));

    let frodo_requests: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::USDC].address.clone(),
            amount: 30_000 * 10i128.pow(6),
        },
    ];
    // Supply frodo tokens
    pool_fixture
        .pool
        .submit(&frodo, &frodo, &frodo, &frodo_requests);
    // Supply and borrow sam tokens
    let sam_requests: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 160_000 * SCALAR_7,
        },
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::WETH].address.clone(),
            amount: 17 * 10i128.pow(9),
        },
        // Sam's max borrow is 39_200 USDC
        Request {
            request_type: 4,
            address: fixture.tokens[TokenIndex::USDC].address.clone(),
            amount: 28_000 * 10i128.pow(6),
        }, // reduces Sam's max borrow to 14_526.31579 USDC
        Request {
            request_type: 4,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 65_000 * SCALAR_7,
        },
    ];
    let sam_positions = pool_fixture
        .pool
        .submit(&samwise, &samwise, &samwise, &sam_requests);
    //Utilization is now:
    // * 36_000 / 40_000 = .9 for USDC
    // * 130_000 / 260_000 = .5 for XLM
    // This equates to the following rough annual interest rates
    //  * 31% for USDC borrowing
    //  * 25.11% for USDC lending
    //  * rate will be dragged up to rate modifier
    //  * 6% for XLM borrowing
    //  * 2.7% for XLM lending

    // Let three months go by and call update every week
    for _ in 0..12 {
        // Let one week pass
        fixture.jump(60 * 60 * 24 * 7);
        // Update emissions
        fixture.emitter.distribute();
        fixture.backstop.update_emission_cycle();
        pool_fixture.pool.update_emissions();
    }
    println!("update emissions");
    // Frodo starts an interest auction
    // type 2 is an interest auction
    let auction_data = pool_fixture.pool.new_auction(&2);
    let usdc_interest_lot_amount = auction_data
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::USDC].address.clone());
    assert_approx_eq_abs(usdc_interest_lot_amount, 256_746831, 5000000);
    let xlm_interest_lot_amount = auction_data
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone());
    assert_approx_eq_abs(xlm_interest_lot_amount, 179_5067018, 5000000);
    let weth_interest_lot_amount = auction_data
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::WETH].address.clone());
    assert_approx_eq_abs(weth_interest_lot_amount, 0_002671545, 5000);
    let usdc_donate_bid_amount = auction_data
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::USDC].address.clone());
    //NOTE: bid USDC amount is seven decimals whereas reserve(and lot) USDC has 6 decomals
    assert_approx_eq_abs(usdc_donate_bid_amount, 392_1769961, SCALAR_7);
    assert_eq!(auction_data.block, 1452403);
    println!("new auction");
    let liq_pct = 3000000;
    let auction_data = pool_fixture
        .pool
        .new_liquidation_auction(&samwise, &liq_pct);
    // println!(
    //     "xlm collateral: {:?}",
    //     sam_positions.collateral.get_unchecked(1)
    // );
    // println!(
    //     "weth collateral: {:?}",
    //     sam_positions.collateral.get_unchecked(2)
    // );
    // println!(
    //     "xlm liability: {:?}",
    //     sam_positions.liabilities.get_unchecked(1)
    // );
    // println!(
    //     "usdc liability {:?}",
    //     sam_positions.liabilities.get_unchecked(0)
    // );
    // //bump actions
    // let frodo_requests: Vec<Request> = vec![
    //     &fixture.env,
    //     Request {
    //         request_type: 2,
    //         address: fixture.tokens[TokenIndex::USDC].address.clone(),
    //         amount: 1,
    //     },
    //     Request {
    //         request_type: 2,
    //         address: fixture.tokens[TokenIndex::XLM].address.clone(),
    //         amount: 1,
    //     },
    //     Request {
    //         request_type: 2,
    //         address: fixture.tokens[TokenIndex::WETH].address.clone(),
    //         amount: 1,
    //     },
    // ];
    // // Supply frodo tokens
    // pool_fixture
    //     .pool
    //     .submit(&frodo, &frodo, &frodo, &frodo_requests);
    fixture.env.as_contract(&pool_fixture.pool.address, || {
        // let key = PoolDataKey::ResData(fixture.tokens[TokenIndex::XLM].address.clone());
        // let xlm_reserve_data = fixture
        //     .env
        //     .storage()
        //     .persistent()
        //     .get::<PoolDataKey, ReserveData>(&key)
        //     .unwrap();
        // println!("xlm d_rate: {:?}", xlm_reserve_data.d_rate);
        // println!("xlm b_rate: {:?}", xlm_reserve_data.b_rate);

        // let key = PoolDataKey::ResData(fixture.tokens[TokenIndex::WETH].address.clone());
        // let weth_reserve_data = fixture
        //     .env
        //     .storage()
        //     .persistent()
        //     .get::<PoolDataKey, ReserveData>(&key)
        //     .unwrap();
        // println!("weth d_rate: {:?}", weth_reserve_data.d_rate);
        // println!("weth b_rate: {:?}", weth_reserve_data.b_rate);
        // let key = PoolDataKey::ResData(fixture.tokens[TokenIndex::USDC].address.clone());
        // let usdc_reserve_data = fixture
        //     .env
        //     .storage()
        //     .persistent()
        //     .get::<PoolDataKey, ReserveData>(&key)
        //     .unwrap();
        // println!("usdc d_rate: {:?}", usdc_reserve_data.d_rate);
        // println!("usdc b_rate: {:?}", weth_reserve_data.b_rate);
        // let usdc_address = fixture
        //     .env
        //     .storage()
        //     .persistent()
        //     .get::<Symbol, Address>(&Symbol::new(&fixture.env, "USDCTkn"))
        //     .unwrap();
        // let bid_amt = auction_data
        //     .bid
        //     .get_unchecked(fixture.tokens[TokenIndex::USDC].address.clone());
        // println!("bid amount: {:?}", bid_amt);
    });

    let usdc_bid_amount = auction_data
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::USDC].address.clone());
    assert_approx_eq_abs(
        usdc_bid_amount,
        sam_positions
            .get_liabilities(0)
            .fixed_mul_ceil(liq_pct, SCALAR_7)
            .unwrap(),
        SCALAR_7,
    );
    let xlm_bid_amount = auction_data
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone());
    assert_approx_eq_abs(
        xlm_bid_amount,
        sam_positions
            .get_liabilities(1)
            .fixed_mul_ceil(liq_pct, SCALAR_7)
            .unwrap(),
        SCALAR_7,
    );
    let xlm_lot_amount = auction_data
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone());
    assert_approx_eq_abs(xlm_lot_amount, 40100_6654560, SCALAR_7);
    let weth_lot_amount = auction_data
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::WETH].address.clone());
    assert_approx_eq_abs(weth_lot_amount, 4_260750195, 1000);

    println!("new liquidation auction");
    //let 100 blocks pass to scale up the modifier
    fixture.jump(101 * 5);

    //fill user and interest liquidation
    let fill_requests = vec![
        &fixture.env,
        Request {
            request_type: 6,
            address: samwise,
            amount: 0,
        },
        Request {
            request_type: 6,
            address: fixture.backstop.address, //address shouldn't matter
            amount: 2,
        },
        Request {
            request_type: 5,
            address: fixture.tokens[TokenIndex::USDC].address.clone(),
            amount: usdc_bid_amount,
        },
    ];
    let frodo_usdc_balance = fixture.tokens[TokenIndex::USDC].balance(&frodo);
    let frodo_xlm_balance = fixture.tokens[TokenIndex::XLM].balance(&frodo);
    let frodo_weth_balance = fixture.tokens[TokenIndex::WETH].balance(&frodo);
    let frodo_positions_post_fill =
        pool_fixture
            .pool
            .submit(&frodo, &frodo, &frodo, &fill_requests);
    println!("filled auctions");
    assert_approx_eq_abs(
        frodo_positions_post_fill.collateral.get_unchecked(2),
        weth_lot_amount + 10 * 10i128.pow(9),
        1000,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.collateral.get_unchecked(1),
        xlm_lot_amount + 100_000 * SCALAR_7,
        1000,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.liabilities.get_unchecked(1),
        xlm_bid_amount + 65_000 * SCALAR_7,
        1000,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.liabilities.get_unchecked(0),
        8_000 * 10i128.pow(6),
        1000,
    );
    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::USDC].balance(&frodo),
        frodo_usdc_balance - 9847_500000
            + usdc_interest_lot_amount
                .fixed_div_floor(2 * 10i128.pow(6), 10i128.pow(6))
                .unwrap()
            - usdc_donate_bid_amount,
        10i128.pow(6),
    );
    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::XLM].balance(&frodo),
        frodo_xlm_balance
            + xlm_interest_lot_amount
                .fixed_div_floor(2 * SCALAR_7, SCALAR_7)
                .unwrap(),
        SCALAR_7,
    );
    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::WETH].balance(&frodo),
        frodo_weth_balance
            + weth_interest_lot_amount
                .fixed_div_floor(2 * 10i128.pow(9), 10i128.pow(9))
                .unwrap(),
        10i128.pow(9),
    );

    // //tank eth price
    // fixture.oracle.set_price(
    //     &fixture.tokens[TokenIndex::WETH].address,
    //     &(500 * 10i128.pow(9)),
    // );
    // //fully liquidate user
    // let sam_usdc_d_tokens = pool_fixture.reserves[0].d_token.balance(&samwise);
    // let sam_xlm_d_tokens = pool_fixture.reserves[1].d_token.balance(&samwise);
    // let sam_b_tokens_weth = sam_b_tokens_weth - expected_weth_amt;
    // let sam_b_tokens_xlm = sam_b_tokens_xlm - expected_xlm_amt;

    // let liq_data_2: LiquidationMetadata = LiquidationMetadata {
    //     collateral: map![
    //         &fixture.env,
    //         (
    //             fixture.tokens[TokenIndex::WETH].address.clone(),
    //             sam_b_tokens_weth
    //         ),
    //         (
    //             fixture.tokens[TokenIndex::XLM].address.clone(),
    //             sam_b_tokens_xlm
    //         )
    //     ],
    //     liability: map![
    //         &fixture.env,
    //         (
    //             fixture.tokens[TokenIndex::USDC].address.clone(),
    //             sam_usdc_d_tokens
    //         ),
    //         (
    //             fixture.tokens[TokenIndex::XLM].address.clone(),
    //             sam_xlm_d_tokens
    //         )
    //     ],
    // };
    // assert_eq!(
    //     sam_b_tokens_weth,
    //     pool_fixture.reserves[2].b_token.balance(&samwise)
    // );
    // let auction_data_2 = pool_fixture
    //     .pool
    //     .new_liquidation_auction(&samwise, &liq_data_2);

    // let usdc_bid_amount = auction_data_2.bid.get_unchecked(0).unwrap();
    // assert_approx_eq_abs(usdc_bid_amount, sam_usdc_d_tokens, SCALAR_7);
    // let xlm_bid_amount = auction_data_2.bid.get_unchecked(1).unwrap();
    // assert_approx_eq_abs(xlm_bid_amount, sam_xlm_d_tokens, SCALAR_7);
    // let xlm_lot_amount = auction_data_2.lot.get_unchecked(1).unwrap();
    // assert_approx_eq_abs(xlm_lot_amount, sam_b_tokens_xlm, SCALAR_7);
    // let weth_lot_amount = auction_data_2.lot.get_unchecked(2).unwrap();
    // assert_approx_eq_abs(weth_lot_amount, sam_b_tokens_weth, 1000);

    // //allow 250 blocks to pass
    // fixture.jump(251 * 5);
    // //fill user liquidation
    // let frodo_xlm_btoken_balance = pool_fixture.reserves[1].b_token.balance(&frodo);
    // let frodo_weth_btoken_balance = pool_fixture.reserves[2].b_token.balance(&frodo);
    // let frodo_usdc_balance = fixture.tokens[TokenIndex::USDC].balance(&frodo);
    // let frodo_xlm_balance = fixture.tokens[TokenIndex::XLM].balance(&frodo);
    // let quote = pool_fixture.pool.fill_auction(&frodo, &0, &samwise);
    // assert_approx_eq_abs(
    //     pool_fixture.reserves[1].b_token.balance(&frodo) - frodo_xlm_btoken_balance,
    //     sam_b_tokens_xlm,
    //     SCALAR_7,
    // );
    // assert_approx_eq_abs(
    //     pool_fixture.reserves[2].b_token.balance(&frodo) - frodo_weth_btoken_balance,
    //     sam_b_tokens_weth,
    //     1000,
    // );
    // assert_approx_eq_abs(
    //     frodo_usdc_balance - fixture.tokens[TokenIndex::USDC].balance(&frodo),
    //     5981_750792,
    //     10i128.pow(6),
    // );
    // assert_approx_eq_abs(
    //     frodo_xlm_balance - fixture.tokens[TokenIndex::XLM].balance(&frodo),
    //     14422_6728800,
    //     SCALAR_7,
    // );
    // let (_, quote_usdc_bid) = quote.bid.get(0).unwrap().unwrap();
    // let (_, quote_xlm_bid) = quote.bid.get(1).unwrap().unwrap();
    // let (_, quote_weth_lot) = quote.lot.get(1).unwrap().unwrap();
    // let (_, quote_xlm_lot) = quote.lot.get(0).unwrap().unwrap();
    // assert_approx_eq_abs(quote_usdc_bid, 5710_0889820, SCALAR_7);
    // assert_approx_eq_abs(quote_xlm_bid, 14290_6823500, SCALAR_7);
    // assert_approx_eq_abs(quote_weth_lot, sam_b_tokens_weth, 1000);
    // assert_approx_eq_abs(quote_xlm_lot, sam_b_tokens_xlm, SCALAR_7);

    // //transfer bad debt to the backstop
    // pool_fixture.pool.bad_debt(&samwise);
    // assert_eq!(pool_fixture.reserves[0].d_token.balance(&samwise), 0);
    // assert_eq!(pool_fixture.reserves[1].d_token.balance(&samwise), 0);
    // assert_eq!(
    //     pool_fixture.reserves[0]
    //         .d_token
    //         .balance(&fixture.backstop.address),
    //     sam_usdc_d_tokens - quote_usdc_bid
    // );
    // assert_eq!(
    //     pool_fixture.reserves[1]
    //         .d_token
    //         .balance(&fixture.backstop.address),
    //     sam_xlm_d_tokens - quote_xlm_bid
    // );

    // // create a bad debt auction
    // let bad_debt_auction_data = pool_fixture.pool.new_auction(&1);
    // assert_eq!(bad_debt_auction_data.bid.len(), 2);
    // assert_eq!(bad_debt_auction_data.lot.len(), 1);
    // assert_eq!(
    //     bad_debt_auction_data.bid.get_unchecked(0).unwrap(),
    //     sam_usdc_d_tokens - quote_usdc_bid
    // );
    // assert_eq!(
    //     bad_debt_auction_data.bid.get_unchecked(1).unwrap(),
    //     sam_xlm_d_tokens - quote_xlm_bid
    // );
    // assert_approx_eq_abs(
    //     bad_debt_auction_data.lot.get_unchecked(u32::MAX).unwrap(),
    //     6929_0835410,
    //     SCALAR_7,
    // );
    // // allow 150 blocks to pass
    // fixture.jump(151 * 5);
    // // fill bad debt auction
    // let frodo_usdc_pre_fill = fixture.tokens[TokenIndex::USDC].balance(&frodo);
    // let frodo_xlm_pre_fill = fixture.tokens[TokenIndex::XLM].balance(&frodo);
    // let frodo_bstop_pre_fill = fixture.tokens[TokenIndex::BSTOP].balance(&frodo);
    // let backstop_bstop_pre_fill =
    //     fixture.tokens[TokenIndex::BSTOP].balance(&fixture.backstop.address);
    // let bad_debt_auction_quote =
    //     pool_fixture
    //         .pool
    //         .fill_auction(&frodo, &1, &fixture.backstop.address);
    // let (_, bad_debt_auction_quote_usdc_bid) = bad_debt_auction_quote.bid.get(0).unwrap().unwrap();
    // let (_, bad_debt_auction_quote_xlm_bid) = bad_debt_auction_quote.bid.get(1).unwrap().unwrap();
    // let (_, bad_debt_auction_quote_lot) = bad_debt_auction_quote.lot.get(0).unwrap().unwrap();
    // assert_eq!(
    //     bad_debt_auction_quote_usdc_bid,
    //     sam_usdc_d_tokens - quote_usdc_bid,
    // );
    // assert_eq!(
    //     pool_fixture.reserves[0]
    //         .d_token
    //         .balance(&fixture.backstop.address),
    //     0,
    // );
    // assert_approx_eq_abs(
    //     frodo_usdc_pre_fill - fixture.tokens[TokenIndex::USDC].balance(&frodo),
    //     1993_916931,
    //     10i128.pow(6),
    // );
    // assert_eq!(
    //     bad_debt_auction_quote_xlm_bid,
    //     sam_xlm_d_tokens - quote_xlm_bid,
    // );
    // assert_eq!(
    //     pool_fixture.reserves[1]
    //         .d_token
    //         .balance(&fixture.backstop.address),
    //     0,
    // );
    // assert_approx_eq_abs(
    //     frodo_xlm_pre_fill - fixture.tokens[TokenIndex::XLM].balance(&frodo),
    //     4807_5576270,
    //     SCALAR_7,
    // );
    // assert_approx_eq_abs(bad_debt_auction_quote_lot, 5196_8126560, SCALAR_7);
    // assert_approx_eq_abs(
    //     fixture.tokens[TokenIndex::BSTOP].balance(&frodo) - frodo_bstop_pre_fill,
    //     5196_8126560,
    //     SCALAR_7,
    // );
    // assert_approx_eq_abs(
    //     backstop_bstop_pre_fill
    //         - fixture.tokens[TokenIndex::BSTOP].balance(&fixture.backstop.address),
    //     5196_8126560,
    //     SCALAR_7,
    // );
    // //check that frodo was correctly slashed
    // let original_deposit = 2_000_000 * SCALAR_7;
    // let pre_withdraw_frodo_bstp = fixture.tokens[TokenIndex::BSTOP].balance(&frodo);
    // fixture
    //     .backstop
    //     .queue_withdrawal(&frodo, &pool_fixture.pool.address, &original_deposit);
    // //jump a month
    // fixture.jump(45 * 24 * 60 * 60);
    // fixture
    //     .backstop
    //     .withdraw(&frodo, &pool_fixture.pool.address, &original_deposit);
    // assert_approx_eq_abs(
    //     fixture.tokens[TokenIndex::BSTOP].balance(&frodo),
    //     pre_withdraw_frodo_bstp + original_deposit - 5196_8126560,
    //     SCALAR_7,
    // );
}
