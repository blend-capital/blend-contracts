#![cfg(test)]

use fixed_point_math::FixedPoint;
use soroban_sdk::{testutils::Address as AddressTestTrait, vec, Address};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    pool::Request,
    test_fixture::{TokenIndex, SCALAR_7, SCALAR_9},
};

/// Smoke test for managing positions, tracking emissions, and accruing interest
#[test]
fn test_wasm_happy_path() {
    let (fixture, frodo) = create_fixture_with_data(true);
    let pool_fixture = &fixture.pools[0];
    let usdc_pool_index = pool_fixture.reserves[&TokenIndex::USDC];
    let xlm_pool_index = pool_fixture.reserves[&TokenIndex::XLM];

    // Create two new users
    let sam = Address::random(&fixture.env); // sam will be supplying XLM and borrowing USDC
    let merry = Address::random(&fixture.env); // merry will be supplying USDC and borrowing XLM

    // Mint users tokens
    let usdc = &fixture.tokens[TokenIndex::USDC];
    let xlm = &fixture.tokens[TokenIndex::XLM];
    let mut sam_usdc_balance = 60_000 * 10i128.pow(6);
    let mut sam_xlm_balance = 2_500_000 * SCALAR_7;
    let mut merry_usdc_balance = 250_000 * 10i128.pow(6);
    let mut merry_xlm_balance = 600_000 * SCALAR_7;
    usdc.mint(&sam, &sam_usdc_balance);
    usdc.mint(&merry, &merry_usdc_balance);
    xlm.mint(&sam, &sam_xlm_balance);
    xlm.mint(&merry, &merry_xlm_balance);

    let mut pool_usdc_balance = usdc.balance(&pool_fixture.pool.address);
    let mut pool_xlm_balance = xlm.balance(&pool_fixture.pool.address);

    let mut sam_xlm_btoken_balance = 0;
    let mut sam_usdc_dtoken_balance = 0;
    let mut merry_usdc_btoken_balance = 0;
    let mut merry_xlm_dtoken_balance = 0;

    // Merry supply USDC
    let amount = 190_000 * 10i128.pow(6);
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                reserve_index: usdc_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    pool_usdc_balance += amount;
    merry_usdc_balance -= amount;
    assert_eq!(usdc.balance(&merry), merry_usdc_balance);
    assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    merry_usdc_btoken_balance += amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.collateral.get_unchecked(usdc_pool_index),
        merry_usdc_btoken_balance,
        10,
    );

    // Sam supply XLM
    let amount = 1_900_000 * SCALAR_7;
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                reserve_index: xlm_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    pool_xlm_balance += amount;
    sam_xlm_balance -= amount;
    assert_eq!(xlm.balance(&sam), sam_xlm_balance);
    assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    sam_xlm_btoken_balance += amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.collateral.get_unchecked(xlm_pool_index),
        sam_xlm_btoken_balance,
        10,
    );

    // Sam borrow USDC
    let amount = 112_000 * 10i128.pow(6); // Sam max borrow is .75*.95*.1*1_900_000 = 135_375 USDC
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 4,
                reserve_index: usdc_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    pool_usdc_balance -= amount;
    sam_usdc_balance += amount;
    assert_eq!(usdc.balance(&sam), sam_usdc_balance);
    assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    sam_usdc_dtoken_balance += amount
        .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.liabilities.get_unchecked(usdc_pool_index),
        sam_usdc_dtoken_balance,
        10,
    );

    // Merry borrow XLM
    let amount = 1_135_000 * SCALAR_7; // Merry max borrow is .75*.9*190_000/.1 = 1_282_5000 XLM
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 4,
                reserve_index: xlm_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    pool_xlm_balance -= amount;
    merry_xlm_balance += amount;
    assert_eq!(xlm.balance(&merry), merry_xlm_balance);
    assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    merry_xlm_dtoken_balance += amount
        .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.liabilities.get_unchecked(xlm_pool_index),
        merry_xlm_dtoken_balance,
        10,
    );

    // Utilization is now:
    // * 120_000 / 200_000 = .625 for USDC
    // * 1_200_000 / 2_000_000 = .625 for XLM
    // This equates to the following rough annual interest rates
    //  * 19.9% for XLM borrowing
    //  * 11.1% for XLM lending
    //  * rate will be dragged up due to rate modifier
    //  * 4.7% for USDC borrowing
    //  * 2.6% for USDC lending
    //  * rate will be dragged down due to rate modifier

    // claim frodo's setup emissions (1h1m passes during setup)
    // - Frodo should receive 60 * 61 * .3 = 1098 BLND from the pool claim
    // - Frodo should receive 60 * 61 * .7 = 2562 BLND from the backstop claim
    let mut frodo_blnd_balance = 0;
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    frodo_blnd_balance += claim_amount;
    assert_eq!(claim_amount, 1098_0000000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_blnd_balance
    );
    fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    frodo_blnd_balance += 2562_0000000;
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_blnd_balance
    );

    // Let three days pass
    fixture.jump(60 * 60 * 24 * 3);

    // Claim 3 day emissions

    // Claim frodo's three day pool emissions
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    frodo_blnd_balance += claim_amount;
    assert_eq!(claim_amount, 4665_6384000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_blnd_balance
    );

    // Claim sam's three day pool emissions
    let mut sam_blnd_balance = 0;
    let claim_amount = pool_fixture
        .pool
        .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    sam_blnd_balance += claim_amount;
    assert_eq!(claim_amount, 730943066650);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&sam),
        sam_blnd_balance
    );

    // Sam repays some of his USDC loan
    let amount = 55_000 * 10i128.pow(6);
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 5,
                reserve_index: usdc_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    pool_usdc_balance += amount;
    sam_usdc_balance -= amount;
    assert_eq!(usdc.balance(&sam), sam_usdc_balance);
    assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    sam_usdc_dtoken_balance -= amount
        .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.liabilities.get_unchecked(usdc_pool_index),
        sam_usdc_dtoken_balance,
        10,
    );

    // Merry repays some of his XLM loan
    let amount = 575_000 * SCALAR_7;
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 5,
                reserve_index: xlm_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    pool_xlm_balance += amount;
    merry_xlm_balance -= amount;
    assert_eq!(xlm.balance(&merry), merry_xlm_balance);
    assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    merry_xlm_dtoken_balance -= amount
        .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.liabilities.get_unchecked(xlm_pool_index),
        merry_xlm_dtoken_balance,
        10,
    );

    // Sam withdraws some of his XLM
    let amount = 1_000_000 * SCALAR_7;
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 3,
                reserve_index: xlm_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    pool_xlm_balance -= amount;
    sam_xlm_balance += amount;
    assert_eq!(xlm.balance(&sam), sam_xlm_balance);
    assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    sam_xlm_btoken_balance -= amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.collateral.get_unchecked(xlm_pool_index),
        sam_xlm_btoken_balance,
        10,
    );

    // Merry withdraws some of his USDC
    let amount = 100_000 * 10i128.pow(6);
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 3,
                reserve_index: usdc_pool_index,
                amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    pool_usdc_balance -= amount;
    merry_usdc_balance += amount;
    assert_eq!(usdc.balance(&merry), merry_usdc_balance);
    assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    merry_usdc_btoken_balance -= amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.collateral.get_unchecked(usdc_pool_index),
        merry_usdc_btoken_balance,
        10,
    );

    // Let one month pass
    fixture.jump(60 * 60 * 24 * 30);

    // Distribute emissions
    fixture.emitter.distribute();
    fixture.backstop.update_emission_cycle();
    pool_fixture.pool.update_emissions();

    // Frodo claim emissions
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    frodo_blnd_balance += claim_amount;
    assert_eq!(claim_amount, 116731656000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_blnd_balance
    );
    fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    frodo_blnd_balance += 420798_0000000;
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_blnd_balance
    );

    // Sam claim emissions
    let claim_amount = pool_fixture
        .pool
        .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    sam_blnd_balance += claim_amount;
    assert_eq!(claim_amount, 90908_8243315);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&sam),
        sam_blnd_balance
    );

    // Let a year go by and call update every week
    for _ in 0..52 {
        // Let one week pass
        fixture.jump(60 * 60 * 24 * 7);
        // Update emissions
        fixture.emitter.distribute();
        fixture.backstop.update_emission_cycle();
        pool_fixture.pool.update_emissions();
    }

    // Frodo claims a year worth of backstop emissions
    fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    frodo_blnd_balance += 22014720_0000000;
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_blnd_balance
    );

    // Frodo claims a year worth of pool emissions
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    frodo_blnd_balance += claim_amount;
    assert_eq!(claim_amount, 1073627_6928000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_blnd_balance
    );

    // Sam claims a year worth of pool emissions
    let claim_amount = pool_fixture
        .pool
        .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    sam_blnd_balance += claim_amount;
    assert_eq!(claim_amount, 8361247_4076689);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&sam),
        sam_blnd_balance
    );

    // Sam repays his USDC loan
    let amount = sam_usdc_dtoken_balance
        .fixed_mul_ceil(1_100_000_000, SCALAR_9)
        .unwrap();
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 5,
                reserve_index: usdc_pool_index,
                amount: amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    let est_amount = sam_usdc_dtoken_balance
        .fixed_mul_ceil(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    pool_usdc_balance += est_amount;
    sam_usdc_balance -= est_amount;
    assert_approx_eq_abs(usdc.balance(&sam), sam_usdc_balance, 100);
    assert_approx_eq_abs(
        usdc.balance(&pool_fixture.pool.address),
        pool_usdc_balance,
        100,
    );
    assert_eq!(result.liabilities.get(usdc_pool_index), None);
    assert_eq!(result.liabilities.len(), 0);

    // Merry repays his XLM loan
    let amount = merry_xlm_dtoken_balance
        .fixed_mul_ceil(1_250_000_000, SCALAR_9)
        .unwrap();
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 5,
                reserve_index: xlm_pool_index,
                amount: amount,
            },
        ],
    );
    let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    let est_amount = merry_xlm_dtoken_balance
        .fixed_mul_ceil(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    pool_xlm_balance += est_amount;
    merry_xlm_balance -= est_amount;
    assert_approx_eq_abs(xlm.balance(&merry), merry_xlm_balance, 100);
    assert_approx_eq_abs(
        xlm.balance(&pool_fixture.pool.address),
        pool_xlm_balance,
        100,
    );
    assert_eq!(result.liabilities.get(xlm_pool_index), None);
    assert_eq!(result.liabilities.len(), 0);

    // Sam withdraws all of his XLM
    let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    let amount = sam_xlm_btoken_balance
        .fixed_mul_ceil(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 3,
                reserve_index: xlm_pool_index,
                amount: amount,
            },
        ],
    );
    pool_xlm_balance -= amount;
    sam_xlm_balance += amount;
    assert_approx_eq_abs(xlm.balance(&sam), sam_xlm_balance, 10);
    assert_approx_eq_abs(
        xlm.balance(&pool_fixture.pool.address),
        pool_xlm_balance,
        10,
    );
    assert_eq!(result.collateral.get(xlm_pool_index), None);

    // Merry withdraws all of his USDC
    let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    let amount = merry_usdc_btoken_balance
        .fixed_mul_ceil(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 3,
                reserve_index: usdc_pool_index,
                amount: amount,
            },
        ],
    );
    pool_usdc_balance -= amount;
    merry_usdc_balance += amount;
    assert_approx_eq_abs(usdc.balance(&merry), merry_usdc_balance, 10);
    assert_approx_eq_abs(
        usdc.balance(&pool_fixture.pool.address),
        pool_usdc_balance,
        10,
    );
    assert_eq!(result.collateral.get(usdc_pool_index), None);

    // Frodo queues for withdrawal a portion of his backstop deposit
    // Backstop shares are still 1 to 1 with BSTOP tokens - no donation via auction or other means has occurred
    let mut frodo_bstop_token_balance = fixture.tokens[TokenIndex::BSTOP].balance(&frodo);
    let mut backstop_bstop_token_balance =
        fixture.tokens[TokenIndex::BSTOP].balance(&fixture.backstop.address);
    let amount = 500 * SCALAR_7;
    let result = fixture
        .backstop
        .queue_withdrawal(&frodo, &pool_fixture.pool.address, &amount);
    assert_eq!(result.amount, amount);
    assert_eq!(
        result.exp,
        fixture.env.ledger().timestamp() + 60 * 60 * 24 * 30
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BSTOP].balance(&frodo),
        frodo_bstop_token_balance
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BSTOP].balance(&fixture.backstop.address),
        backstop_bstop_token_balance
    );

    // Time passes and Frodo withdraws his queued for withdrawal backstop deposit
    fixture.jump(60 * 60 * 24 * 30 + 1);
    let result = fixture
        .backstop
        .withdraw(&frodo, &pool_fixture.pool.address, &amount);
    frodo_bstop_token_balance += result;
    backstop_bstop_token_balance -= result;
    assert_eq!(result, amount);
    assert_eq!(
        fixture.tokens[TokenIndex::BSTOP].balance(&frodo),
        frodo_bstop_token_balance
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BSTOP].balance(&fixture.backstop.address),
        backstop_bstop_token_balance
    );
}
