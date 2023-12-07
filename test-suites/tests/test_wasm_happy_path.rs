#![cfg(test)]

use pool::Request;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{testutils::Address as _, vec, Address};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7, SCALAR_9},
};

/// Smoke test for managing positions, tracking emissions, and accruing interest
#[test]
fn test_wasm_happy_path() {
    let fixture = create_fixture_with_data(true);
    let frodo = fixture.users.get(0).unwrap();
    let pool_fixture = &fixture.pools[0];
    let stable_pool_index = pool_fixture.reserves[&TokenIndex::STABLE];
    let xlm_pool_index = pool_fixture.reserves[&TokenIndex::XLM];

    // Create two new users
    let sam = Address::generate(&fixture.env); // sam will be supplying XLM and borrowing STABLE
    let merry = Address::generate(&fixture.env); // merry will be supplying STABLE and borrowing XLM

    // Mint users tokens
    let stable = &fixture.tokens[TokenIndex::STABLE];
    let xlm = &fixture.tokens[TokenIndex::XLM];
    let mut sam_stable_balance = 60_000 * 10i128.pow(6);
    let mut sam_xlm_balance = 2_500_000 * SCALAR_7;
    let mut merry_stable_balance = 250_000 * 10i128.pow(6);
    let mut merry_xlm_balance = 600_000 * SCALAR_7;
    stable.mint(&sam, &sam_stable_balance);
    stable.mint(&merry, &merry_stable_balance);
    xlm.mint(&sam, &sam_xlm_balance);
    xlm.mint(&merry, &merry_xlm_balance);

    let mut pool_stable_balance = stable.balance(&pool_fixture.pool.address);
    let mut pool_xlm_balance = xlm.balance(&pool_fixture.pool.address);

    let mut sam_xlm_btoken_balance = 0;
    let mut sam_stable_dtoken_balance = 0;
    let mut merry_stable_btoken_balance = 0;
    let mut merry_xlm_dtoken_balance = 0;

    // Merry supply STABLE
    let amount = 190_000 * 10i128.pow(6);
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                address: stable.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::STABLE);
    pool_stable_balance += amount;
    merry_stable_balance -= amount;
    assert_eq!(stable.balance(&merry), merry_stable_balance);
    assert_eq!(
        stable.balance(&pool_fixture.pool.address),
        pool_stable_balance
    );
    merry_stable_btoken_balance += amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.collateral.get_unchecked(stable_pool_index),
        merry_stable_btoken_balance,
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
                address: xlm.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
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

    // Sam borrow STABLE
    let amount = 112_000 * 10i128.pow(6); // Sam max borrow is .75*.95*.1*1_900_000 = 135_375 STABLE
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 4,
                address: stable.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::STABLE);
    pool_stable_balance -= amount;
    sam_stable_balance += amount;
    assert_eq!(stable.balance(&sam), sam_stable_balance);
    assert_eq!(
        stable.balance(&pool_fixture.pool.address),
        pool_stable_balance
    );
    sam_stable_dtoken_balance += amount
        .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.liabilities.get_unchecked(stable_pool_index),
        sam_stable_dtoken_balance,
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
                address: xlm.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
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
    // * 120_000 / 200_000 = .625 for STABLE
    // * 1_200_000 / 2_000_000 = .625 for XLM
    // This equates to the following rough annual interest rates
    //  * 19.9% for XLM borrowing
    //  * 11.1% for XLM lending
    //  * rate will be dragged up due to rate modifier
    //  * 4.7% for STABLE borrowing
    //  * 2.6% for STABLE lending
    //  * rate will be dragged down due to rate modifier

    // claim frodo's setup emissions (1h1m passes during setup)
    // - Frodo should receive 60 * 61 * .3 = 1098 BLND from the pool claim
    // - Frodo should receive 60 * 61 * .7 = 2562 BLND from the backstop claim
    let mut backstop_blnd_balance =
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address);
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(claim_amount, 1098_0000000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );
    let claim_amount = fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    assert_eq!(claim_amount, 2562_0000000);
    backstop_blnd_balance -= 2562_0000000;
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );

    // Let three days pass
    fixture.jump(60 * 60 * 24 * 3);

    // Claim 3 day emissions

    // Claim frodo's three day pool emissions
    let frodo_balance = fixture.tokens[TokenIndex::BLND].balance(&frodo);
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(claim_amount, 4665_6384000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_balance + claim_amount
    );

    // Claim sam's three day pool emissions
    let sam_balance = fixture.tokens[TokenIndex::BLND].balance(&sam);
    let claim_amount = pool_fixture
        .pool
        .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(claim_amount, 730943066650);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&sam),
        sam_balance + claim_amount
    );

    // Sam repays some of his STABLE loan
    let amount = 55_000 * 10i128.pow(6);
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 5,
                address: stable.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::STABLE);
    pool_stable_balance += amount;
    sam_stable_balance -= amount;
    assert_eq!(stable.balance(&sam), sam_stable_balance);
    assert_eq!(
        stable.balance(&pool_fixture.pool.address),
        pool_stable_balance
    );
    sam_stable_dtoken_balance -= amount
        .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.liabilities.get_unchecked(stable_pool_index),
        sam_stable_dtoken_balance,
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
                address: xlm.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
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
                address: xlm.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
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

    // Merry withdraws some of his STABLE
    let amount = 100_000 * 10i128.pow(6);
    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 3,
                address: stable.address.clone(),
                amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::STABLE);
    pool_stable_balance -= amount;
    merry_stable_balance += amount;
    assert_eq!(stable.balance(&merry), merry_stable_balance);
    assert_eq!(
        stable.balance(&pool_fixture.pool.address),
        pool_stable_balance
    );
    merry_stable_btoken_balance -= amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.collateral.get_unchecked(stable_pool_index),
        merry_stable_btoken_balance,
        10,
    );

    // Let rest of emission period pass
    fixture.jump(341940);

    // Distribute emissions
    fixture.emitter.distribute();
    fixture.backstop.gulp_emissions();
    pool_fixture.pool.gulp_emissions();

    // Frodo claim emissions
    let mut backstop_blnd_balance =
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address);
    let frodo_balance = fixture.tokens[TokenIndex::BLND].balance(&frodo);
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(claim_amount, 11673_1656000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&frodo),
        frodo_balance + claim_amount
    );

    let claim_amount = fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    assert_eq!(claim_amount, 420798_0000000);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );

    // Sam claim emissions
    let sam_balance = fixture.tokens[TokenIndex::BLND].balance(&sam);
    let claim_amount = pool_fixture
        .pool
        .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(claim_amount, 90908_8243315);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&sam),
        sam_balance + claim_amount
    );

    // Let 51 weeks go by and call update to validate emissions won't get missed
    fixture.jump(60 * 60 * 24 * 7 * 51);
    fixture.emitter.distribute();
    fixture.backstop.gulp_emissions();
    pool_fixture.pool.gulp_emissions();
    // Allow another week go by to distribute missed emissions
    fixture.jump(60 * 60 * 24 * 7);
    fixture.emitter.distribute();
    fixture.backstop.gulp_emissions();
    pool_fixture.pool.gulp_emissions();

    // Frodo claims a year worth of backstop emissions
    let mut backstop_blnd_balance =
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address);
    let claim_amount = fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    assert_eq!(claim_amount, 22014719_9998450); //actual amount is 22014720_0000000 but get's rounded down // 22014719_9998450
    backstop_blnd_balance -= 22014719_9998450;
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );

    // Frodo claims a year worth of pool emissions
    let claim_amount = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(claim_amount, 1073628_1728000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );

    // Sam claims a year worth of pool emissions
    let claim_amount = pool_fixture
        .pool
        .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    backstop_blnd_balance -= claim_amount;
    assert_eq!(claim_amount, 8361251_7312500);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        backstop_blnd_balance
    );

    // Sam repays his STABLE loan
    let amount = sam_stable_dtoken_balance
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
                address: stable.address.clone(),
                amount: amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::STABLE);
    let est_amount = sam_stable_dtoken_balance
        .fixed_mul_ceil(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    pool_stable_balance += est_amount;
    sam_stable_balance -= est_amount;
    assert_approx_eq_abs(stable.balance(&sam), sam_stable_balance, 100);
    assert_approx_eq_abs(
        stable.balance(&pool_fixture.pool.address),
        pool_stable_balance,
        100,
    );
    assert_eq!(result.liabilities.get(stable_pool_index), None);
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
                address: xlm.address.clone(),
                amount: amount,
            },
        ],
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
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
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
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
                address: xlm.address.clone(),
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

    // Merry withdraws all of his STABLE
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::STABLE);
    let amount = merry_stable_btoken_balance
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
                address: stable.address.clone(),
                amount: amount,
            },
        ],
    );
    pool_stable_balance -= amount;
    merry_stable_balance += amount;
    assert_approx_eq_abs(stable.balance(&merry), merry_stable_balance, 10);
    assert_approx_eq_abs(
        stable.balance(&pool_fixture.pool.address),
        pool_stable_balance,
        10,
    );
    assert_eq!(result.collateral.get(stable_pool_index), None);

    // Frodo queues for withdrawal a portion of his backstop deposit
    // Backstop shares are still 1 to 1 with BSTOP tokens - no donation via auction or other means has occurred
    let mut frodo_bstop_token_balance = fixture.lp.balance(&frodo);
    let mut backstop_bstop_token_balance = fixture.lp.balance(&fixture.backstop.address);
    let amount = 500 * SCALAR_7;
    let result = fixture
        .backstop
        .queue_withdrawal(&frodo, &pool_fixture.pool.address, &amount);
    assert_eq!(result.amount, amount);
    assert_eq!(
        result.exp,
        fixture.env.ledger().timestamp() + 60 * 60 * 24 * 30
    );
    assert_eq!(fixture.lp.balance(&frodo), frodo_bstop_token_balance);
    assert_eq!(
        fixture.lp.balance(&fixture.backstop.address),
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
    assert_eq!(fixture.lp.balance(&frodo), frodo_bstop_token_balance);
    assert_eq!(
        fixture.lp.balance(&fixture.backstop.address),
        backstop_bstop_token_balance
    );
}
