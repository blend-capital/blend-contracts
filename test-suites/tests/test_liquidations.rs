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
    fixture.tokens[TokenIndex::XLM as usize].mint(
        &fixture.bombadil,
        &samwise,
        &(100_000 * SCALAR_7),
    );
    fixture.tokens[TokenIndex::WETH as usize].mint(&fixture.bombadil, &samwise, &(5 * SCALAR_7));
    // Supply tokens
    let frodo_b_tokens = pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].contract_id,
        &(10_000 * SCALAR_7),
    );
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&frodo),
        88_000 * SCALAR_7
    );
    assert!((frodo_b_tokens < (10_000 * SCALAR_7)) & (frodo_b_tokens > (9_999 * SCALAR_7)));

    let sam_b_tokens = pool_fixture.pool.supply(
        &samwise,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &(80_000 * SCALAR_7),
    );
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&samwise),
        20_000 * SCALAR_7
    );
    println!("sam_b_tokens: {}", sam_b_tokens);
    assert!((sam_b_tokens < (80_000 * SCALAR_7)) & (sam_b_tokens > (79_998 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[1].b_token.balance(&samwise),
        sam_b_tokens
    );
    let sam_b_tokens = pool_fixture.pool.supply(
        &samwise,
        &fixture.tokens[TokenIndex::WETH as usize].contract_id,
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
        &fixture.tokens[TokenIndex::USDC as usize].contract_id,
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
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
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
        fixture.backstop.dist();
        pool_fixture.pool.updt_emis();
        // let d_tokens = pool_fixture.pool.borrow(
        //     &frodo,
        //     &fixture.tokens[TokenIndex::USDC as usize].contract_id,
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
        //     &fixture.tokens[TokenIndex::USDC as usize].contract_id,
        //     &SCALAR_7,
        //     &frodo,
        // );
    }
    let d_tokens = pool_fixture.pool.borrow(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].contract_id,
        &(SCALAR_7),
        &frodo,
    );
    println!("");
    println!("USDC d_tokens {}", d_tokens);
    let d_rate = SCALAR_7.fixed_div_floor(d_tokens, SCALAR_7).unwrap() - SCALAR_7;
    println!("USDC borrow interest accrued {}", d_rate);
    pool_fixture.pool.repay(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].contract_id,
        &SCALAR_7,
        &frodo,
    );
    let d_tokens_1 = pool_fixture.pool.borrow(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &(SCALAR_7),
        &frodo,
    );
    println!("");
    println!("XLM d_tokens {}", d_tokens_1);
    let d_rate_1 = SCALAR_7.fixed_div_floor(d_tokens_1, SCALAR_7).unwrap() - SCALAR_7;
    println!("XLM borrow interest accrued {}", d_rate_1);
    pool_fixture.pool.repay(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &SCALAR_7,
        &frodo,
    );
    let b_tokens = pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &(SCALAR_7),
    );
    println!("");
    println!("xlm b_tokens {}", b_tokens);
    let b_rate = SCALAR_7.fixed_div_floor(b_tokens, SCALAR_7).unwrap() - SCALAR_7;
    println!("XLM supply interest accrued {}", b_rate);
    pool_fixture.pool.withdraw(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &SCALAR_7,
        &frodo,
    );
    let b_tokens_1 = pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::WETH as usize].contract_id,
        &(SCALAR_7),
    );
    println!("");
    println!("weth b_tokens {}", b_tokens_1);
    let b_rate_1 = SCALAR_7.fixed_div_floor(b_tokens_1, SCALAR_7).unwrap() - SCALAR_7;
    println!("weth supply interest accrued {}", b_rate_1);
    pool_fixture.pool.withdraw(
        &frodo,
        &fixture.tokens[TokenIndex::WETH as usize].contract_id,
        &SCALAR_7,
        &frodo,
    );
    // fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
    //     let key = PoolDataKey::ResData(
    //         fixture.tokens[TokenIndex::USDC as usize]
    //             .contract_id
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
    let auction_data = pool_fixture.pool.new_auct(&2);
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
                fixture.tokens[TokenIndex::WETH as usize]
                    .contract_id
                    .clone(),
                500_0000
            ),
            (
                fixture.tokens[TokenIndex::XLM as usize].contract_id.clone(),
                22500 * SCALAR_7
            )
        ],
        liability: map![
            &fixture.env,
            (
                fixture.tokens[TokenIndex::USDC as usize]
                    .contract_id
                    .clone(),
                2500 * SCALAR_7
            ),
            (
                fixture.tokens[TokenIndex::XLM as usize].contract_id.clone(),
                3963 * SCALAR_7
            )
        ],
    };
    let auction_data = pool_fixture.pool.new_liq_a(&samwise, &liq_data);
    println!("auction data {:?}", auction_data);
}
