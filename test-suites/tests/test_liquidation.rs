#![cfg(test)]
use cast::i128;
use pool::{PoolDataKey, Positions, Request, ReserveConfig, ReserveData};
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Events},
    vec, Address, Error, IntoVal, Symbol, Val, Vec,
};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7},
};

#[test]
fn test_liquidations() {
    let fixture = create_fixture_with_data(false);
    let frodo = fixture.users.get(0).unwrap();
    let pool_fixture = &fixture.pools[0];

    // Disable rate modifiers
    let mut usdc_config: ReserveConfig = fixture.read_reserve_config(0, TokenIndex::STABLE);
    usdc_config.reactivity = 0;
    pool_fixture
        .pool
        .update_reserve(&fixture.tokens[TokenIndex::STABLE].address, &usdc_config);
    let mut xlm_config: ReserveConfig = fixture.read_reserve_config(0, TokenIndex::XLM);
    xlm_config.reactivity = 0;
    pool_fixture
        .pool
        .update_reserve(&fixture.tokens[TokenIndex::XLM].address, &xlm_config);
    let mut weth_config: ReserveConfig = fixture.read_reserve_config(0, TokenIndex::WETH);
    weth_config.reactivity = 0;
    pool_fixture
        .pool
        .update_reserve(&fixture.tokens[TokenIndex::WETH].address, &weth_config);

    // Create a user
    let samwise = Address::generate(&fixture.env); //sam will be supplying XLM and borrowing STABLE

    // Mint users tokens
    fixture.tokens[TokenIndex::XLM].mint(&samwise, &(500_000 * SCALAR_7));
    fixture.tokens[TokenIndex::WETH].mint(&samwise, &(50 * 10i128.pow(9)));
    fixture.tokens[TokenIndex::USDC].mint(&frodo, &(100_000 * SCALAR_7));

    let frodo_requests: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
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
        // Sam's max borrow is 39_200 STABLE
        Request {
            request_type: 4,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: 28_000 * 10i128.pow(6),
        }, // reduces Sam's max borrow to 14_526.31579 STABLE
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
    // * 36_000 / 40_000 = .9 for STABLE
    // * 130_000 / 260_000 = .5 for XLM
    // This equates to the following rough annual interest rates
    //  * 31% for STABLE borrowing
    //  * 25.11% for STABLE lending
    //  * rate will be dragged up to rate modifier
    //  * 6% for XLM borrowing
    //  * 2.7% for XLM lending

    // Let three months go by and call update every week
    for _ in 0..12 {
        // Let one week pass
        fixture.jump(60 * 60 * 24 * 7);
        // Update emissions
        fixture.emitter.distribute();
        fixture.backstop.gulp_emissions();
        pool_fixture.pool.gulp_emissions();
    }
    // Start an interest auction
    // type 2 is an interest auction
    let auction_type: u32 = 2;
    let auction_data = pool_fixture.pool.new_auction(&auction_type);
    let stable_interest_lot_amount = auction_data
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::STABLE].address.clone());
    assert_approx_eq_abs(stable_interest_lot_amount, 256_746831, 5000000);
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
    //NOTE: bid STABLE amount is seven decimals whereas reserve(and lot) STABLE has 6 decomals
    assert_approx_eq_abs(usdc_donate_bid_amount, 392_1769961, SCALAR_7);
    assert_eq!(auction_data.block, 151);
    let liq_pct = 30;
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 1)];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (Symbol::new(&fixture.env, "new_auction"), auction_type).into_val(&fixture.env),
                auction_data.into_val(&fixture.env)
            )
        ]
    );
    // Start a liquidation auction
    let auction_data = pool_fixture
        .pool
        .new_liquidation_auction(&samwise, &liq_pct);

    let usdc_bid_amount = auction_data
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::STABLE].address.clone());
    assert_approx_eq_abs(
        usdc_bid_amount,
        sam_positions
            .liabilities
            .get(0)
            .unwrap()
            .fixed_mul_ceil(i128(liq_pct * 100000), SCALAR_7)
            .unwrap(),
        SCALAR_7,
    );
    let xlm_bid_amount = auction_data
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone());
    assert_approx_eq_abs(
        xlm_bid_amount,
        sam_positions
            .liabilities
            .get(1)
            .unwrap()
            .fixed_mul_ceil(i128(liq_pct * 100000), SCALAR_7)
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
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 1)];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "new_liquidation_auction"),
                    samwise.clone()
                )
                    .into_val(&fixture.env),
                auction_data.into_val(&fixture.env)
            )
        ]
    );

    //let 100 blocks pass to scale up the modifier
    fixture.jump_with_sequence(101 * 5);
    //fill user and interest liquidation
    let auct_type_1: u32 = 0;
    let auct_type_2: u32 = 2;
    let fill_requests = vec![
        &fixture.env,
        Request {
            request_type: 6,
            address: samwise.clone(),
            amount: 25,
        },
        Request {
            request_type: 6,
            address: samwise.clone(),
            amount: 100,
        },
        Request {
            request_type: 8,
            address: fixture.backstop.address.clone(), //address shouldn't matter
            amount: 99,
        },
        Request {
            request_type: 8,
            address: fixture.backstop.address.clone(), //address shouldn't matter
            amount: 100,
        },
        Request {
            request_type: 5,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: usdc_bid_amount,
        },
    ];
    let frodo_stable_balance = fixture.tokens[TokenIndex::STABLE].balance(&frodo);
    let frodo_xlm_balance = fixture.tokens[TokenIndex::XLM].balance(&frodo);
    let frodo_weth_balance = fixture.tokens[TokenIndex::WETH].balance(&frodo);
    let frodo_positions_post_fill =
        pool_fixture
            .pool
            .submit(&frodo, &frodo, &frodo, &fill_requests);
    assert_approx_eq_abs(
        frodo_positions_post_fill.collateral.get_unchecked(2),
        weth_lot_amount
            .fixed_div_floor(2_0000000, SCALAR_7)
            .unwrap()
            + 10 * 10i128.pow(9),
        1000,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.collateral.get_unchecked(1),
        xlm_lot_amount.fixed_div_floor(2_0000000, SCALAR_7).unwrap() + 100_000 * SCALAR_7,
        1000,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.liabilities.get_unchecked(1),
        xlm_bid_amount + 65_000 * SCALAR_7,
        1000,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.liabilities.get_unchecked(0),
        8_000 * 10i128.pow(6) + 559_285757,
        100000,
    );
    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::STABLE].balance(&frodo),
        frodo_stable_balance - usdc_bid_amount
            + stable_interest_lot_amount
                .fixed_div_floor(2 * 10i128.pow(6), 10i128.pow(6))
                .unwrap(), // - usdc_donate_bid_amount TODO: add donate diff when donating is implemented
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
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 16)];
    let fill_pct_1: i128 = 25;
    let fill_pct_2: i128 = 100;
    let fill_pct_3: i128 = 99;
    let event_data_1: Vec<Val> = vec![
        &fixture.env,
        frodo.into_val(&fixture.env),
        fill_pct_1.into_val(&fixture.env),
    ];
    let event_data_2: Vec<Val> = vec![
        &fixture.env,
        frodo.into_val(&fixture.env),
        fill_pct_2.into_val(&fixture.env),
    ];
    let event_data_3: Vec<Val> = vec![
        &fixture.env,
        frodo.into_val(&fixture.env),
        fill_pct_3.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "fill_auction"),
                    samwise.clone(),
                    auct_type_1
                )
                    .into_val(&fixture.env),
                event_data_1.into_val(&fixture.env)
            )
        ]
    );
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 15)];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "fill_auction"),
                    samwise.clone(),
                    auct_type_1
                )
                    .into_val(&fixture.env),
                event_data_2.into_val(&fixture.env)
            )
        ]
    );
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 9)];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "fill_auction"),
                    fixture.backstop.address.clone(),
                    auct_type_2
                )
                    .into_val(&fixture.env),
                event_data_3.into_val(&fixture.env)
            )
        ]
    );
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 3)];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "fill_auction"),
                    fixture.backstop.address.clone(),
                    auct_type_2
                )
                    .into_val(&fixture.env),
                event_data_2.into_val(&fixture.env)
            )
        ]
    );

    //tank eth price
    fixture.oracle.set_price_stable(&vec![
        &fixture.env,
        500_0000000, // eth
        1_0000000,   // usdc
        0_1000000,   // xlm
        1_0000000,   // stable
    ]);

    //fully liquidate user
    let blank_requests: Vec<Request> = vec![&fixture.env];
    pool_fixture
        .pool
        .submit(&samwise, &samwise, &samwise, &blank_requests);
    let liq_pct = 100;
    let auction_data_2 = pool_fixture
        .pool
        .new_liquidation_auction(&samwise, &liq_pct);

    let usdc_bid_amount = auction_data_2
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::STABLE].address.clone());
    assert_approx_eq_abs(usdc_bid_amount, 19599_872330, 100000);
    let xlm_bid_amount = auction_data_2
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone());
    assert_approx_eq_abs(xlm_bid_amount, 45498_8226700, SCALAR_7);
    let xlm_lot_amount = auction_data_2
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone());
    assert_approx_eq_abs(xlm_lot_amount, 139947_2453890, SCALAR_7);
    let weth_lot_amount = auction_data_2
        .lot
        .get_unchecked(fixture.tokens[TokenIndex::WETH].address.clone());
    assert_approx_eq_abs(weth_lot_amount, 14_869584990, 100000000);

    //allow 250 blocks to pass
    fixture.jump_with_sequence(251 * 5);
    //fill user liquidation
    let frodo_stable_balance = fixture.tokens[TokenIndex::STABLE].balance(&frodo);
    let frodo_xlm_balance = fixture.tokens[TokenIndex::XLM].balance(&frodo);
    let fill_requests = vec![
        &fixture.env,
        Request {
            request_type: 6,
            address: samwise.clone(),
            amount: 100,
        },
        Request {
            request_type: 5,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: usdc_bid_amount
                .fixed_div_floor(2_0000000, SCALAR_7)
                .unwrap(),
        },
        Request {
            request_type: 5,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: xlm_bid_amount.fixed_div_floor(2_0000000, SCALAR_7).unwrap(),
        },
    ];
    let usdc_filled = usdc_bid_amount
        .fixed_mul_floor(3_0000000, SCALAR_7)
        .unwrap()
        .fixed_div_floor(4_0000000, SCALAR_7)
        .unwrap();
    let xlm_filled = xlm_bid_amount
        .fixed_mul_floor(3_0000000, SCALAR_7)
        .unwrap()
        .fixed_div_floor(4_0000000, SCALAR_7)
        .unwrap();
    let new_frodo_positions = pool_fixture
        .pool
        .submit(&frodo, &frodo, &frodo, &fill_requests);
    assert_approx_eq_abs(
        frodo_positions_post_fill.collateral.get(1).unwrap() + xlm_lot_amount,
        new_frodo_positions.collateral.get(1).unwrap(),
        SCALAR_7,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.collateral.get(2).unwrap() + weth_lot_amount,
        new_frodo_positions.collateral.get(2).unwrap(),
        SCALAR_7,
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.liabilities.get(0).unwrap() + usdc_filled - 9147_499950,
        new_frodo_positions.liabilities.get(0).unwrap(),
        10i128.pow(6),
    );
    assert_approx_eq_abs(
        frodo_positions_post_fill.liabilities.get(1).unwrap() + xlm_filled - 22438_6298700,
        new_frodo_positions.liabilities.get(1).unwrap(),
        SCALAR_7,
    );
    assert_approx_eq_abs(
        frodo_stable_balance - 9799_936164,
        fixture.tokens[TokenIndex::STABLE].balance(&frodo),
        10i128.pow(6),
    );
    assert_approx_eq_abs(
        frodo_xlm_balance - 22749_4113400,
        fixture.tokens[TokenIndex::XLM].balance(&frodo),
        SCALAR_7,
    );

    //transfer bad debt to the backstop
    let blank_request: Vec<Request> = vec![&fixture.env];
    let samwise_positions_pre_bd =
        pool_fixture
            .pool
            .submit(&samwise, &samwise, &samwise, &blank_request);
    pool_fixture.pool.bad_debt(&samwise);
    let backstop_positions = pool_fixture.pool.submit(
        &fixture.backstop.address,
        &fixture.backstop.address,
        &fixture.backstop.address,
        &blank_request,
    );
    assert_eq!(
        samwise_positions_pre_bd.liabilities.get(0).unwrap(),
        backstop_positions.liabilities.get(0).unwrap()
    );
    assert_eq!(
        samwise_positions_pre_bd.liabilities.get(1).unwrap(),
        backstop_positions.liabilities.get(1).unwrap()
    );

    // create a bad debt auction
    let auction_type: u32 = 1;
    let bad_debt_auction_data = pool_fixture.pool.new_auction(&auction_type);

    assert_eq!(bad_debt_auction_data.bid.len(), 2);
    assert_eq!(bad_debt_auction_data.lot.len(), 1);

    assert_eq!(
        bad_debt_auction_data
            .bid
            .get_unchecked(fixture.tokens[TokenIndex::STABLE].address.clone()),
        samwise_positions_pre_bd.liabilities.get(0).unwrap() //d rate 1.071330239
    );
    assert_eq!(
        bad_debt_auction_data
            .bid
            .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone()),
        samwise_positions_pre_bd.liabilities.get(1).unwrap() //d rate 1.013853805
    );
    assert_approx_eq_abs(
        bad_debt_auction_data
            .lot
            .get_unchecked(fixture.lp.address.clone()),
        7171_0435309, // lp_token value is $1.25 each
        SCALAR_7,
    );
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 1)];

    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (Symbol::new(&fixture.env, "new_auction"), auction_type).into_val(&fixture.env),
                bad_debt_auction_data.into_val(&fixture.env)
            )
        ]
    );
    // allow 100 blocks to pass
    fixture.jump_with_sequence(101 * 5);
    // fill bad debt auction
    let frodo_bstop_pre_fill = fixture.lp.balance(&frodo);
    let backstop_bstop_pre_fill = fixture.lp.balance(&fixture.backstop.address);
    let auction_type: u32 = 1;
    let bad_debt_fill_request = vec![
        &fixture.env,
        Request {
            request_type: 7,
            address: fixture.backstop.address.clone(),
            amount: 20,
        },
    ];
    let post_bd_fill_frodo_positions =
        pool_fixture
            .pool
            .submit(&frodo, &frodo, &frodo, &bad_debt_fill_request);

    assert_eq!(
        post_bd_fill_frodo_positions.liabilities.get(0).unwrap(),
        new_frodo_positions.liabilities.get(0).unwrap()
            + samwise_positions_pre_bd
                .liabilities
                .get(0)
                .unwrap()
                .fixed_mul_ceil(20, 100)
                .unwrap(),
    );
    assert_eq!(
        post_bd_fill_frodo_positions.liabilities.get(1).unwrap(),
        new_frodo_positions.liabilities.get(1).unwrap()
            + samwise_positions_pre_bd
                .liabilities
                .get(1)
                .unwrap()
                .fixed_mul_ceil(20, 100)
                .unwrap(),
    );
    assert_approx_eq_abs(
        fixture.lp.balance(&frodo),
        frodo_bstop_pre_fill + 717_1043530,
        SCALAR_7,
    );
    assert_approx_eq_abs(
        fixture.lp.balance(&fixture.backstop.address),
        backstop_bstop_pre_fill - 717_1043531,
        SCALAR_7,
    );
    let new_auction = pool_fixture
        .pool
        .get_auction(&(1 as u32), &fixture.backstop.address);
    assert_eq!(new_auction.bid.len(), 2);
    assert_eq!(new_auction.lot.len(), 1);
    assert_eq!(
        new_auction
            .bid
            .get_unchecked(fixture.tokens[TokenIndex::STABLE].address.clone()),
        samwise_positions_pre_bd
            .liabilities
            .get(0)
            .unwrap()
            .fixed_mul_floor(80, 100)
            .unwrap()
    );
    assert_eq!(
        new_auction
            .bid
            .get_unchecked(fixture.tokens[TokenIndex::XLM].address.clone()),
        samwise_positions_pre_bd
            .liabilities
            .get(1)
            .unwrap()
            .fixed_mul_floor(80, 100)
            .unwrap()
    );
    assert_approx_eq_abs(
        new_auction.lot.get_unchecked(fixture.lp.address.clone()),
        bad_debt_auction_data
            .lot
            .get_unchecked(fixture.lp.address.clone())
            - 1434_2087060,
        SCALAR_7,
    );
    assert_eq!(new_auction.block, bad_debt_auction_data.block);
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 1)];
    let fill_pct: i128 = 20;
    let event_data: Vec<Val> = vec![
        &fixture.env,
        frodo.into_val(&fixture.env),
        fill_pct.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "fill_auction"),
                    fixture.backstop.address.clone(),
                    auction_type
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );
    // allow another 50 blocks to pass (150 total)
    fixture.jump_with_sequence(50 * 5);
    // fill bad debt auction
    let frodo_bstop_pre_fill = fixture.lp.balance(&frodo);
    let backstop_bstop_pre_fill = fixture.lp.balance(&fixture.backstop.address);
    let auction_type: u32 = 1;
    let bad_debt_fill_request = vec![
        &fixture.env,
        Request {
            request_type: 7,
            address: fixture.backstop.address.clone(),
            amount: 100,
        },
    ];
    let post_bd_fill_frodo_positions =
        pool_fixture
            .pool
            .submit(&frodo, &frodo, &frodo, &bad_debt_fill_request);

    assert_eq!(
        post_bd_fill_frodo_positions.liabilities.get(0).unwrap(),
        new_frodo_positions.liabilities.get(0).unwrap()
            + samwise_positions_pre_bd.liabilities.get(0).unwrap(),
    );
    assert_eq!(
        post_bd_fill_frodo_positions.liabilities.get(1).unwrap(),
        new_frodo_positions.liabilities.get(1).unwrap()
            + samwise_positions_pre_bd.liabilities.get(1).unwrap(),
    );
    assert_approx_eq_abs(
        fixture.lp.balance(&frodo),
        frodo_bstop_pre_fill + 4302_6261190,
        SCALAR_7,
    );
    assert_approx_eq_abs(
        fixture.lp.balance(&fixture.backstop.address),
        backstop_bstop_pre_fill - 4302_6261190,
        SCALAR_7,
    );
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 1)];
    let fill_pct: i128 = 100;
    let event_data: Vec<Val> = vec![
        &fixture.env,
        frodo.into_val(&fixture.env),
        fill_pct.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "fill_auction"),
                    fixture.backstop.address.clone(),
                    auction_type
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );

    //check that frodo was correctly slashed
    let original_deposit = 50_000 * SCALAR_7;
    let pre_withdraw_frodo_bstp = fixture.lp.balance(&frodo);
    fixture
        .backstop
        .queue_withdrawal(&frodo, &pool_fixture.pool.address, &(original_deposit));
    //jump a month
    fixture.jump(45 * 24 * 60 * 60);
    fixture
        .backstop
        .withdraw(&frodo, &pool_fixture.pool.address, &original_deposit);
    assert_approx_eq_abs(
        fixture.lp.balance(&frodo) - pre_withdraw_frodo_bstp,
        original_deposit - 717_1043530 - 4302_6261190,
        SCALAR_7,
    );
    fixture
        .backstop
        .deposit(&frodo, &pool_fixture.pool.address, &10_0000000);

    // Test bad debt was burned correctly
    // Sam re-borrows
    let sam_requests: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::WETH].address.clone(),
            amount: 1 * 10i128.pow(9),
        },
        // Sam's max borrow is 39_200 STABLE
        Request {
            request_type: 4,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: 100 * 10i128.pow(6),
        }, // reduces Sam's max borrow to 14_526.31579 STABLE
    ];
    let sam_positions = pool_fixture
        .pool
        .submit(&samwise, &samwise, &samwise, &sam_requests);

    // Nuke eth price more
    fixture.oracle.set_price_stable(&vec![
        &fixture.env,
        10_0000000, // eth
        1_0000000,  // usdc
        0_1000000,  // xlm
        1_0000000,  // stable
    ]);

    // Liquidate sam
    let liq_pct: u64 = 100;
    let auction_data = pool_fixture
        .pool
        .new_liquidation_auction(&samwise, &liq_pct);
    let usdc_bid_amount = auction_data
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::STABLE].address.clone());
    assert_approx_eq_abs(
        usdc_bid_amount,
        sam_positions
            .liabilities
            .get(0)
            .unwrap()
            .fixed_mul_ceil(i128(liq_pct * 100000), SCALAR_7)
            .unwrap(),
        SCALAR_7,
    );
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 1)];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "new_liquidation_auction"),
                    samwise.clone()
                )
                    .into_val(&fixture.env),
                auction_data.into_val(&fixture.env)
            )
        ]
    );
    //jump 400 blocks
    fixture.jump_with_sequence(401 * 5);
    //fill liq
    let bad_debt_fill_request = vec![
        &fixture.env,
        Request {
            request_type: 6,
            address: samwise.clone(),
            amount: 100,
        },
    ];

    pool_fixture
        .pool
        .submit(&frodo, &frodo, &frodo, &bad_debt_fill_request);
    // transfer bad debt to backstop
    pool_fixture.pool.bad_debt(&samwise);

    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 1)];
    let bad_debt: i128 = 92903018;
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (Symbol::new(&fixture.env, "bad_debt"), samwise.clone()).into_val(&fixture.env),
                vec![
                    &fixture.env,
                    fixture.tokens[TokenIndex::STABLE].address.clone().to_val(),
                    bad_debt.into_val(&fixture.env),
                ]
                .into_val(&fixture.env)
            )
        ]
    );

    // Create bad debt auction
    let auction_type: u32 = 1;
    pool_fixture.pool.new_auction(&auction_type);

    //fill bad debt auction
    fixture.jump_with_sequence(401 * 5);
    let bump_usdc = vec![
        &fixture.env,
        Request {
            request_type: 4,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: 1,
        },
    ];
    let frodo_positions = pool_fixture.pool.submit(&frodo, &frodo, &frodo, &bump_usdc);
    // check bad debt
    fixture.env.as_contract(&pool_fixture.pool.address, || {
        let key = PoolDataKey::Positions(fixture.backstop.address.clone());
        let positions = fixture
            .env
            .storage()
            .persistent()
            .get::<PoolDataKey, Positions>(&key)
            .unwrap();
        assert_eq!(positions.liabilities.len(), 1);
        assert_eq!(positions.liabilities.get(0).unwrap(), bad_debt);
    });
    // check d_supply
    let d_supply = 19104604033;
    fixture.env.as_contract(&pool_fixture.pool.address, || {
        let key = PoolDataKey::ResData(fixture.tokens[TokenIndex::STABLE].address.clone());
        let data = fixture
            .env
            .storage()
            .persistent()
            .get::<PoolDataKey, ReserveData>(&key)
            .unwrap();
        assert_eq!(data.d_supply, d_supply);
    });
    let bad_debt_fill_request = vec![
        &fixture.env,
        Request {
            request_type: 7,
            address: fixture.backstop.address.clone(),
            amount: 100,
        },
    ];
    let post_bd_fill_frodo_positions =
        pool_fixture
            .pool
            .submit(&frodo, &frodo, &frodo, &bad_debt_fill_request);
    assert_eq!(
        frodo_positions.liabilities.get(0),
        post_bd_fill_frodo_positions.liabilities.get(0)
    );
    fixture.env.as_contract(&pool_fixture.pool.address, || {
        let key = PoolDataKey::Positions(fixture.backstop.address.clone());
        let positions = fixture
            .env
            .storage()
            .persistent()
            .get::<PoolDataKey, Positions>(&key)
            .unwrap();
        assert_eq!(positions.liabilities.len(), 0);
        let key = PoolDataKey::ResData(fixture.tokens[TokenIndex::STABLE].address.clone());
        let data = fixture
            .env
            .storage()
            .persistent()
            .get::<PoolDataKey, ReserveData>(&key)
            .unwrap();
        assert_eq!(data.d_supply, d_supply - bad_debt);
    });
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 2)];
    let bad_debt: i128 = 92903018;
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "bad_debt"),
                    fixture.backstop.address.clone()
                )
                    .into_val(&fixture.env),
                vec![
                    &fixture.env,
                    fixture.tokens[TokenIndex::STABLE].address.clone().to_val(),
                    bad_debt.into_val(&fixture.env),
                ]
                .into_val(&fixture.env)
            )
        ]
    );
}

#[test]
fn test_user_restore_position_and_delete_liquidation() {
    let fixture = create_fixture_with_data(false);
    let pool_fixture = &fixture.pools[0];
    let stable_pool_index = pool_fixture.reserves[&TokenIndex::STABLE];
    let xlm_pool_index = pool_fixture.reserves[&TokenIndex::XLM];

    // Create a user that is supply STABLE (cf = 90%, $1) and borrowing XLM (lf = 75%, $0.10)
    let samwise = Address::generate(&fixture.env);
    fixture.tokens[TokenIndex::STABLE].mint(&samwise, &(1100 * 10i128.pow(6)));

    // deposit $1k stable and borrow to 90% borrow limit ($810)
    let setup_request: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: 1000 * 10i128.pow(6),
        },
        Request {
            request_type: 4,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 6075 * SCALAR_7,
        },
    ];
    pool_fixture
        .pool
        .submit(&samwise, &samwise, &samwise, &setup_request);

    // simulate 20% XLM price increase ($972 liabilities, $900 limit) and create user liquidation
    fixture.oracle.set_price_stable(&vec![
        &fixture.env,
        2000_0000000, // eth
        1_0000000,    // usdc
        0_1200000,    // xlm
        1_0000000,    // stable
    ]);
    pool_fixture.pool.new_liquidation_auction(&samwise, &50);
    assert!(pool_fixture.pool.try_get_auction(&0, &samwise).is_ok());

    // jump 200 blocks
    fixture.jump_with_sequence(200 * 5);

    // validate liquidation can't be deleted without restoring position
    let delete_only_request: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 9,
            address: Address::generate(&fixture.env),
            amount: i128::MAX,
        },
    ];
    let delete_only =
        pool_fixture
            .pool
            .try_submit(&samwise, &samwise, &samwise, &delete_only_request);
    assert_eq!(delete_only.err(), Some(Ok(Error::from_contract_error(10))));

    // validate health factor must be fully restored before deleting position
    let short_supply_delete_request: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: 79 * 10i128.pow(6), // need $80 more collateral
        },
        Request {
            request_type: 9,
            address: Address::generate(&fixture.env),
            amount: i128::MAX,
        },
    ];
    let short_supply_delete =
        pool_fixture
            .pool
            .try_submit(&samwise, &samwise, &samwise, &short_supply_delete_request);
    assert_eq!(
        short_supply_delete.err(),
        Some(Ok(Error::from_contract_error(10)))
    );

    let short_repay_delete_request: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 9,
            address: Address::generate(&fixture.env),
            amount: i128::MAX,
        },
        Request {
            request_type: 5,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 449 * SCALAR_7, // need to repay 450 XLM
        },
    ];
    let short_repay_delete =
        pool_fixture
            .pool
            .try_submit(&samwise, &samwise, &samwise, &short_repay_delete_request);
    assert_eq!(
        short_repay_delete.err(),
        Some(Ok(Error::from_contract_error(10)))
    );

    // validate liquidation can be deleted after restoring position
    let delete_request: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: 41 * 10i128.pow(6),
        },
        Request {
            request_type: 9,
            address: Address::generate(&fixture.env),
            amount: i128::MAX,
        },
        Request {
            request_type: 5,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 226 * SCALAR_7,
        },
    ];
    let sam_positions = pool_fixture
        .pool
        .submit(&samwise, &samwise, &samwise, &delete_request);
    // fuzz assert wide to account for b and d rates (only verify actions occurred)
    assert_approx_eq_abs(
        sam_positions.collateral.get_unchecked(stable_pool_index),
        1041 * 10i128.pow(6),
        10000,
    );
    assert_approx_eq_abs(
        sam_positions.liabilities.get_unchecked(xlm_pool_index),
        5849 * SCALAR_7,
        SCALAR_7,
    );
    assert!(pool_fixture.pool.try_get_auction(&0, &samwise).is_err());
}
