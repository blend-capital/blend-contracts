#![cfg(test)]
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, testutils::Address as AddressTestTrait, vec, Address};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    pool::{AuctionData, LiquidationMetadata, PoolDataKey, ReserveData},
    test_fixture::{TokenIndex, SCALAR_7},
};

#[test]
fn test_liquidations() {
    let (fixture, frodo) = create_fixture_with_data();
    let pool_fixture = &fixture.pools[0];

    // Create a user
    let samwise = Address::random(&fixture.env); //sam will be supplying XLM and borrowing USDC

    // Mint users tokens
    fixture.tokens[TokenIndex::XLM as usize].mint(&samwise, &(100_000 * SCALAR_7));
    fixture.tokens[TokenIndex::WETH as usize].mint(&samwise, &(5 * SCALAR_7));
    // Supply tokens
    let frodo_b_tokens = pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(10_000 * SCALAR_7),
    );
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&frodo),
        88_000 * SCALAR_7
    );
    assert_approx_eq_abs(frodo_b_tokens, 10_000 * SCALAR_7, SCALAR_7);

    let sam_b_tokens = pool_fixture.pool.supply(
        &samwise,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(80_000 * SCALAR_7),
    );
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&samwise),
        20_000 * SCALAR_7
    );
    println!("sam_b_tokens: {}", sam_b_tokens);
    assert_approx_eq_abs(sam_b_tokens, 80_000 * SCALAR_7, 2 * SCALAR_7);
    assert_eq!(
        pool_fixture.reserves[1].b_token.balance(&samwise),
        sam_b_tokens
    );
    let sam_b_tokens = pool_fixture.pool.supply(
        &samwise,
        &fixture.tokens[TokenIndex::WETH as usize].address,
        &(5 * SCALAR_7),
    );
    assert_eq!(
        fixture.tokens[TokenIndex::WETH as usize].balance(&samwise),
        0
    );
    assert!((sam_b_tokens < (5 * SCALAR_7)) & (sam_b_tokens > (4 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[2].b_token.balance(&samwise),
        sam_b_tokens
    );
    // Borrow tokens
    let sam_d_tokens = pool_fixture.pool.borrow(
        &samwise,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(10_000 * SCALAR_7),
        &samwise,
    ); //sams max USDC is .75*.95*.1*80_000 + .8*.95*2_000*5 = 13_300 USDC
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&samwise),
        10_000 * SCALAR_7
    );
    assert!((sam_d_tokens < (10_000 * SCALAR_7)) & (sam_d_tokens > (9_999 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[0].d_token.balance(&samwise),
        sam_d_tokens
    );
    let sam_d_tokens = pool_fixture.pool.borrow(
        &samwise,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(25_000 * SCALAR_7),
        &samwise,
    ); //sams max XLM borrow is (.75*.1*80_000 + .8*2_000*5 - 10_000/.95)*.75/.1 = 26_052_6315800 XLM
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&samwise),
        45_000 * SCALAR_7
    );
    assert!((sam_d_tokens < (25_000 * SCALAR_7)) & (sam_d_tokens > (24_998 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[1].d_token.balance(&samwise),
        sam_d_tokens
    );
    //Utilization is now:
    // * 18_000 / 20_000 = .9 for USDC
    // * 90_000 / 180_000 = .5 for XLM
    // This equates to the following rough annual interest rates
    //  * 31% for USDC borrowing
    //  * 25.11% for USDC lending
    //  * rate will be dragged up to rate modifier
    //  * 6% for XLM borrowing
    //  * 2.7% for XLM lending

    // Let two months go by and call update every week
    for i in 0..8 {
        // Let one week pass
        fixture.jump(60 * 60 * 24 * 7);
        // Update emissions
        fixture.emitter.distribute();
        fixture.backstop.distribute();
        pool_fixture.pool.update_emissions();
        // let d_tokens = pool_fixture.pool.borrow(
        //     &frodo,
        //     &fixture.tokens[TokenIndex::USDC as usize].address,
        //     &(SCALAR_7),
        //     &frodo,
        // );
        // println!("");
        // println!("week {}", i);
        // println!("USDC d_tokens {}", d_tokens);
        // let interest_accrued = SCALAR_7.fixed_div_floor(d_tokens, SCALAR_7).unwrap() - SCALAR_7;
        // println!("USDC interest accrued {}", interest_accrued);
        // pool_fixture.pool.repay(
        //     &frodo,
        //     &fixture.tokens[TokenIndex::USDC as usize].address,
        //     &SCALAR_7,
        //     &frodo,
        // );//
    }
    let d_tokens = pool_fixture.pool.borrow(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(SCALAR_7),
        &frodo,
    );
    println!("");
    println!("USDC d_tokens {}", d_tokens);
    let d_rate = SCALAR_7.fixed_div_floor(d_tokens, SCALAR_7).unwrap() - SCALAR_7;
    println!("USDC borrow interest accrued {}", d_rate);
    pool_fixture.pool.repay(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &SCALAR_7,
        &frodo,
    );
    let d_tokens_1 = pool_fixture.pool.borrow(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(SCALAR_7),
        &frodo,
    );
    println!("");
    println!("XLM d_tokens {}", d_tokens_1);
    let d_rate_1 = SCALAR_7.fixed_div_floor(d_tokens_1, SCALAR_7).unwrap() - SCALAR_7;
    println!("XLM borrow interest accrued {}", d_rate_1);
    pool_fixture.pool.repay(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &SCALAR_7,
        &frodo,
    );
    let b_tokens = pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(SCALAR_7),
    );
    println!("");
    println!("xlm b_tokens {}", b_tokens);
    let b_rate = SCALAR_7.fixed_div_floor(b_tokens, SCALAR_7).unwrap() - SCALAR_7;
    println!("XLM supply interest accrued {}", b_rate);
    pool_fixture.pool.withdraw(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &SCALAR_7,
        &frodo,
    );
    let b_tokens_1 = pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::WETH as usize].address,
        &(SCALAR_7),
    );
    println!("");
    println!("weth b_tokens {}", b_tokens_1);
    let b_rate_1 = SCALAR_7.fixed_div_floor(b_tokens_1, SCALAR_7).unwrap() - SCALAR_7;
    println!("weth supply interest accrued {}", b_rate_1);
    pool_fixture.pool.withdraw(
        &frodo,
        &fixture.tokens[TokenIndex::WETH as usize].address,
        &SCALAR_7,
        &frodo,
    );
    // fixture.env.as_contract(&pool_fixture.pool.address, || {
    //     let key = PoolDataKey::ResData(
    //         fixture.tokens[TokenIndex::USDC as usize]
    //             .address
    //             .clone(),
    //     );
    //     let data = fixture
    //         .env
    //         .storage()
    //         .get::<PoolDataKey, ReserveData>(&key)
    //         .unwrap()
    //         .unwrap();
    //     println!("d_tokens {}", data.d_supply);
    //     println!("rate modifier {}", data.ir_mod);
    // });

    // Frodo starts an interest auction
    // type 2 is an interest auction
    let auction_data = pool_fixture.pool.new_auction(&2);
    let usdc_lot_amount = auction_data.lot.get_unchecked(0).unwrap();
    assert_approx_eq_abs(usdc_lot_amount, 85_8461538, 5000000);
    let xlm_lot_amount = auction_data.lot.get_unchecked(1).unwrap();
    assert_approx_eq_abs(xlm_lot_amount, 83_0769231, 5000000);
    let weth_lot_amount = auction_data.lot.get_unchecked(2).unwrap();
    assert_approx_eq_abs(weth_lot_amount, 0_0025989, 5000000);
    let usdc_bid_amount = auction_data.bid.get(u32::MAX).unwrap().unwrap();
    assert_approx_eq_abs(usdc_bid_amount, 143_7824001, 5000000);
    assert_eq!(auction_data.block, 968563);
    // Assumed max liquidation:

    let liq_data: LiquidationMetadata = LiquidationMetadata {
        collateral: map![
            &fixture.env,
            (
                fixture.tokens[TokenIndex::WETH as usize].address.clone(),
                2 * SCALAR_7
            ),
            (
                fixture.tokens[TokenIndex::XLM as usize].address.clone(),
                22500 * SCALAR_7
            )
        ],
        liability: map![
            &fixture.env,
            (
                fixture.tokens[TokenIndex::USDC as usize].address.clone(),
                2500 * SCALAR_7
            ),
            (
                fixture.tokens[TokenIndex::XLM as usize].address.clone(),
                6000 * SCALAR_7
            )
        ],
    };
    let auction_data = pool_fixture
        .pool
        .new_liquidation_auction(&samwise, &liq_data);
    let usdc_bid_amount = auction_data.bid.get_unchecked(0).unwrap();
    assert_approx_eq_abs(usdc_bid_amount, 2386_4828850, SCALAR_7);
    let xlm_bid_amount = auction_data.bid.get_unchecked(1).unwrap();
    assert_approx_eq_abs(xlm_bid_amount, 5945_1099880, SCALAR_7);
    let xlm_lot_amount = auction_data.lot.get_unchecked(1).unwrap();
    assert_approx_eq_abs(xlm_lot_amount, 22406_8680900, SCALAR_7);
    let weth_lot_amount = auction_data.lot.get_unchecked(2).unwrap();
    assert_approx_eq_abs(weth_lot_amount, 1_9968884, 1000);

    //let 100 blocks pass to scale up the modifier
    fixture.jump(101 * 5);

    //fill user liquidation
    let frodo_xlm_btoken_balance = pool_fixture.reserves[1].b_token.balance(&frodo);
    let frodo_weth_btoken_balance = pool_fixture.reserves[2].b_token.balance(&frodo);
    let frodo_usdc_balance = fixture.tokens[TokenIndex::USDC as usize].balance(&frodo);
    let frodo_xlm_balance = fixture.tokens[TokenIndex::XLM as usize].balance(&frodo);
    let quote = pool_fixture.pool.fill_auction(&frodo, &0, &samwise);
    assert_approx_eq_abs(
        pool_fixture.reserves[1].b_token.balance(&frodo) - frodo_xlm_btoken_balance,
        22406_8680900
            .fixed_div_floor(2 * SCALAR_7, SCALAR_7)
            .unwrap(),
        SCALAR_7,
    );
    assert_approx_eq_abs(
        pool_fixture.reserves[2].b_token.balance(&frodo) - frodo_weth_btoken_balance,
        1_9968884.fixed_div_floor(2 * SCALAR_7, SCALAR_7).unwrap(),
        1000,
    );
    assert_approx_eq_abs(
        frodo_usdc_balance - fixture.tokens[TokenIndex::USDC as usize].balance(&frodo),
        2500 * SCALAR_7,
        SCALAR_7,
    );
    assert_approx_eq_abs(
        frodo_xlm_balance - fixture.tokens[TokenIndex::XLM as usize].balance(&frodo),
        6000 * SCALAR_7,
        SCALAR_7,
    );
    let (_, quote_usdc_bid) = quote.bid.get(0).unwrap().unwrap();
    let (_, quote_xlm_bid) = quote.bid.get(1).unwrap().unwrap();
    let (_, quote_weth_lot) = quote.lot.get(1).unwrap().unwrap();
    let (_, quote_xlm_lot) = quote.lot.get(0).unwrap().unwrap();
    assert_approx_eq_abs(quote_usdc_bid, 2386_4828850, SCALAR_7);
    assert_approx_eq_abs(quote_xlm_bid, 5945_1099880, SCALAR_7);
    assert_approx_eq_abs(
        quote_weth_lot,
        1_9968884.fixed_div_floor(2 * SCALAR_7, SCALAR_7).unwrap(),
        1000,
    );
    assert_approx_eq_abs(
        quote_xlm_lot,
        22406_8680900
            .fixed_div_floor(2 * SCALAR_7, SCALAR_7)
            .unwrap(),
        SCALAR_7,
    );
    //tank eth price
    fixture.oracle.set_price(
        &fixture.tokens[TokenIndex::WETH as usize].address,
        &500_0000000,
    );
    //fully liquidate user
    let liq_data_2: LiquidationMetadata = LiquidationMetadata {
        collateral: map![
            &fixture.env,
            (
                fixture.tokens[TokenIndex::WETH as usize].address.clone(),
                4 * SCALAR_7
            ),
            (
                fixture.tokens[TokenIndex::XLM as usize].address.clone(),
                69040 * SCALAR_7
            )
        ],
        liability: map![
            &fixture.env,
            (
                fixture.tokens[TokenIndex::USDC as usize].address.clone(),
                8000 * SCALAR_7
            ),
            (
                fixture.tokens[TokenIndex::XLM as usize].address.clone(),
                20000 * SCALAR_7
            )
        ],
    };
    let auction_data_2 = pool_fixture
        .pool
        .new_liquidation_auction(&samwise, &liq_data_2);
}
