#![cfg(test)]

use fixed_point_math::FixedPoint;
use lending_pool::Request;
use soroban_sdk::{testutils::Address as _, vec, Address, Vec};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7, SCALAR_9},
};

/// Smoke test for managing positions, tracking emissions, and accruing interest
#[test]
fn test_wasm_happy_path() {
    let (fixture, frodo) = create_fixture_with_data(true);
    let pool_fixture = &fixture.pools[0];
    let usdc_pool_index = pool_fixture.reserves[&TokenIndex::STABLE];
    let xlm_pool_index = pool_fixture.reserves[&TokenIndex::XLM];
    // Create users
    let sam: Address = Address::random(&fixture.env); // GD7ST2GLWBLNQKAMGN5QCM42KKVO3KO2FSTS3M47HBD2FOGTTUBJ6M6P
    let merry = Address::random(&fixture.env); // GA5XD47THVXOJFNSQTOYBIO42EVGY5NF62YUAZJNHOQFWZZ2EEITVI5K
    let pippin = Address::random(&fixture.env); // GBK2NUW53IMDR6UUB4HS66QLMKBKAXKRII5KN3PMGP7MTJHLSPMHBY5X
    let gimli = Address::random(&fixture.env); //GDKNKDIIBBMAH3TKQTD4NQGXHVX35V6MB6YNXKLZCJ6ACZBYIKGJ2A6I
    let legolas = Address::random(&fixture.env); // GD3NKCDMMIQMKMWHJCKOKWRV7HSLJ5U5DGC7CARCOOGX53HINXTBPY3X

    let requests: Vec<(Address, i128, i128, i128)> = vec![
        &fixture.env,
        (sam.clone(), 4, 1692045721, 1000000000),
        (sam.clone(), 5, 1692045966, 1000000000),
        (sam.clone(), 5, 1692045976, 0),
        (sam.clone(), 4, 1692046412, 6321952446),
        (sam.clone(), 4, 1692046515, 30989743),
        (sam.clone(), 5, 1692046701, 6352942189),
        (sam.clone(), 4, 1692046754, 7112195301),
        (sam.clone(), 2, 1692046771, 7112195301),
        (sam.clone(), 4, 1692046898, 5500000000),
        (sam.clone(), 2, 1692046908, 5500000000),
        (sam.clone(), 4, 1692046966, 5000000000),
        (sam.clone(), 2, 1692046977, 5000000000),
        (sam.clone(), 4, 1692046999, 4000000000),
        (sam.clone(), 2, 1692047030, 4000000000),
        (sam.clone(), 3, 1692047081, 4229151747),
        (sam.clone(), 5, 1692047091, 4229151747),
        (sam.clone(), 3, 1692047102, 5070035341),
        (sam.clone(), 5, 1692047113, 5070035341),
        (sam.clone(), 3, 1692047123, 6078112208),
        (sam.clone(), 5, 1692047134, 6078112208),
        (sam.clone(), 3, 1692047150, 6234896614),
        (sam.clone(), 5, 1692047161, 6234896614),
        (sam.clone(), 4, 1692047451, 3000000000),
        (sam.clone(), 5, 1692047851, 3000000000),
        (merry.clone(), 5, 1692103688, 10000000000),
        (sam.clone(), 5, 1692128012, 0),
        (sam.clone(), 4, 1692130243, 3000000000),
        (sam.clone(), 5, 1692131231, 3000000000),
        (sam.clone(), 5, 1692132894, 6903),
        (pippin.clone(), 4, 1692143044, 2500000000),
        (pippin.clone(), 4, 1692143832, 4640000000),
        (pippin.clone(), 2, 1692143848, 7140000000),
        (pippin.clone(), 4, 1692143873, 5950000000),
        (pippin.clone(), 2, 1692143932, 5950000000),
        (merry.clone(), 2, 1692193661, 100000000000),
        (merry.clone(), 2, 1692711888, 500000000000),
        (merry.clone(), 3, 1692712225, 20000000000),
        (gimli.clone(), 4, 1692818684, 10000000),
        (gimli.clone(), 4, 1692819485, 69268557),
        (gimli.clone(), 4, 1692821826, 4872699819),
        (gimli.clone(), 4, 1692885926, 200000000),
        (merry.clone(), 2, 1692986474, 10000000),
        (merry.clone(), 4, 1692986693, 10000000),
        (merry.clone(), 4, 1692986816, 1000000000),
        (legolas.clone(), 4, 1693326387, 790000000),
        (merry.clone(), 2, 1693839178, 150000000000),
        (gimli.clone(), 5, 1693841456, 50000000),
    ];
    // match whale stuff
    let xlm = &fixture.tokens[TokenIndex::XLM];
    pool_fixture.pool.submit(
        &frodo,
        &frodo,
        &frodo,
        &vec![
            &fixture.env,
            Request {
                request_type: 5,
                address: xlm.address.clone(),
                amount: 60_000 * SCALAR_7,
            },
            Request {
                request_type: 3,
                address: xlm.address.clone(),
                amount: 90_000 * SCALAR_7,
            },
        ],
    );

    // Mint users tokens
    let usdc = &fixture.tokens[TokenIndex::STABLE];
    let mut sam_usdc_balance = 600_000_000_000 * 10i128.pow(6);
    usdc.mint(&sam, &sam_usdc_balance);
    xlm.mint(&sam, &(100_000 * SCALAR_7));

    let mut merry_usdc_balance = 600_000_000_000 * 10i128.pow(6);
    usdc.mint(&merry, &sam_usdc_balance);
    xlm.mint(&merry, &(100_000 * SCALAR_7));

    let mut pippin_usdc_balance = 600_000_000_000 * 10i128.pow(6);
    usdc.mint(&pippin, &sam_usdc_balance);
    xlm.mint(&pippin, &(100_000 * SCALAR_7));

    let mut gimli_usdc_balance = 600_000_000_000 * 10i128.pow(6);
    usdc.mint(&gimli, &sam_usdc_balance);
    xlm.mint(&gimli, &(100_000 * SCALAR_7));

    let mut legolas_usdc_balance = 600_000_000_000 * 10i128.pow(6);
    usdc.mint(&legolas, &sam_usdc_balance);
    xlm.mint(&legolas, &(100_000 * SCALAR_7));

    let mut pool_usdc_balance = usdc.balance(&pool_fixture.pool.address);
    let mut pool_xlm_balance = xlm.balance(&pool_fixture.pool.address);

    let mut sam_xlm_btoken_balance = 0;
    let mut sam_usdc_dtoken_balance = 0;
    let mut merry_usdc_btoken_balance = 0;
    let mut merry_xlm_dtoken_balance = 0;

    // users supply usdc
    let amount = 600_00_000_000 * 10i128.pow(6);
    let result = pool_fixture.pool.submit(
        &sam,
        &sam,
        &sam,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                address: usdc.address.clone(),
                amount,
            },
        ],
    );

    let result = pool_fixture.pool.submit(
        &merry,
        &merry,
        &merry,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                address: usdc.address.clone(),
                amount,
            },
        ],
    );
    // let merry_xlm_liabilities = result.liabilities.get_unchecked(xlm_pool_index);
    // println!("merry_xlm_liabilities: {}", merry_xlm_liabilities);

    let result = pool_fixture.pool.submit(
        &pippin,
        &pippin,
        &pippin,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                address: usdc.address.clone(),
                amount,
            },
        ],
    );

    let result = pool_fixture.pool.submit(
        &gimli,
        &gimli,
        &gimli,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                address: usdc.address.clone(),
                amount,
            },
        ],
    );

    let result = pool_fixture.pool.submit(
        &legolas,
        &legolas,
        &legolas,
        &vec![
            &fixture.env,
            Request {
                request_type: 2,
                address: usdc.address.clone(),
                amount,
            },
        ],
    );

    // Users carry out XLM requests
    let (_, _, last_timestamp, _) = requests.get(0).unwrap();
    for request in requests {
        let (user, action, timestamp, amount) = request;
        fixture.jump(timestamp as u64 - last_timestamp as u64);
        let user_balance = xlm.balance(&user);
        let result = pool_fixture.pool.submit(
            &user,
            &user,
            &user,
            &vec![
                &fixture.env,
                Request {
                    request_type: action as u32,
                    address: xlm.address.clone(),
                    amount,
                },
            ],
        );

        let xlm_data = pool_fixture.pool.get_reserve_data(&xlm.address);
        println!("");
        println!("timestamp: {}", timestamp);
        println!("usdc_d_rate: {:?}", xlm_data.d_rate);
        println!("usdc_b_rate: {:?}", xlm_data.b_rate);
        println!("usdc d supply: {:?}", xlm_data.d_supply);
        println!("usdc b supply: {:?}", xlm_data.b_supply);
        println!("backstop credit: {:?}", xlm_data.backstop_credit);
        println!("ir_mod: {:?}", xlm_data.ir_mod);
        println!("amt: {:?}", amount);
        let balance_diff = user_balance - xlm.balance(&user);
        println!("balance diff {:?}", balance_diff);
        if (action != 4 && action != 5) {
            let new_b_tokens =
                result.collateral.get_unchecked(xlm_pool_index) - sam_xlm_btoken_balance;
            sam_xlm_btoken_balance = result.collateral.get_unchecked(xlm_pool_index);
            println!("new_b_tokens: {}", new_b_tokens);
        } else {
            // let xlm_liabilities = result.liabilities.get_unchecked(xlm_pool_index);
            // println!("xlm_liabilities: {}", xlm_liabilities);
        }
    }
    // let amount = 192_000 * 10i128.pow(6);
    // let result = pool_fixture.pool.submit(
    //     &sam,
    //     &sam,
    //     &sam,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 4,
    //             address: usdc.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // pool_usdc_balance -= amount;
    // sam_usdc_balance += amount;
    // assert_eq!(usdc.balance(&sam), sam_usdc_balance);
    // assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    // sam_usdc_dtoken_balance += amount
    //     .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.liabilities.get_unchecked(usdc_pool_index),
    //     sam_usdc_dtoken_balance,
    //     10,
    // );
    // println!("sam_usdc_dtoken_balance: {}", sam_usdc_dtoken_balance);
    // println!("usdc left {:?}", usdc.balance(&pool_fixture.pool.address));

    // let usdc_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // println!("usdc_d_rate: {:?}", usdc_data.d_rate);
    // println!("usdc_b_rate: {:?}", usdc_data.b_rate);
    // println!("usdc d supply: {:?}", usdc_data.d_supply);
    // println!("usdc b supply: {:?}", usdc_data.b_supply);
    // println!("backstop credit: {:?}", usdc_data.backstop_credit);

    // // Merry borrow XLM
    // let amount = 1_135_000 * SCALAR_7; // Merry max borrow is .75*.9*190_000/.1 = 1_282_5000 XLM
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 4,
    //             address: xlm.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    // pool_xlm_balance -= amount;
    // merry_xlm_balance += amount;
    // assert_eq!(xlm.balance(&merry), merry_xlm_balance);
    // assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    // merry_xlm_dtoken_balance += amount
    //     .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.liabilities.get_unchecked(xlm_pool_index),
    //     merry_xlm_dtoken_balance,
    //     10,
    // );

    // Utilization is now:
    // * 120_000 / 200_000 = 1.000 for STABLE
    // * 1_200_000 / 2_000_000 = .625 for XLM
    // This equates to the following rough annual interest rates
    //  * 19.9% for XLM borrowing
    //  * 11.1% for XLM lending
    //  * rate will be dragged up due to rate modifier

    // claim frodo's setup emissions (1h1m passes during setup)
    // - Frodo should receive 60 * 61 * .3 = 1098 BLND from the pool claim
    // - Frodo should receive 60 * 61 * .7 = 2562 BLND from the backstop claim
    // let mut frodo_blnd_balance = fixture.tokens[TokenIndex::BLND].balance(&frodo);
    // let claim_amount = pool_fixture
    //     .pool
    //     .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    // frodo_blnd_balance += claim_amount;
    // assert_eq!(claim_amount, 1098_0000000);
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND].balance(&frodo),
    //     frodo_blnd_balance
    // );
    // fixture.backstop.claim(
    //     &frodo,
    //     &vec![&fixture.env, pool_fixture.pool.address.clone()],
    //     &frodo,
    // );
    // frodo_blnd_balance += 2562_0000000;
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND].balance(&frodo),
    //     frodo_blnd_balance
    // );

    // Let seven days pass
    fixture.jump(60 * 60 * 24 * 300);
    // let amount = 100 * 10i128.pow(6);
    // let result = pool_fixture.pool.submit(
    //     &sam,
    //     &sam,
    //     &sam,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 4,
    //             address: usdc.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // pool_usdc_balance -= amount;
    // sam_usdc_balance += amount;
    // assert_eq!(usdc.balance(&sam), sam_usdc_balance);
    // assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    // sam_usdc_dtoken_balance += amount
    //     .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.liabilities.get_unchecked(usdc_pool_index),
    //     sam_usdc_dtoken_balance,
    //     10,
    // );
    // println!("sam_usdc_dtoken_balance 2: {}", sam_usdc_dtoken_balance);
    // let amount = 192_000 * 10i128.pow(6);
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 2,
    //             address: usdc.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // pool_usdc_balance += amount;
    // merry_usdc_balance -= amount;
    // assert_eq!(usdc.balance(&merry), merry_usdc_balance);
    // assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    // merry_usdc_btoken_balance += amount
    //     .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.collateral.get_unchecked(usdc_pool_index),
    //     merry_usdc_btoken_balance,
    //     10,
    // );
    // println!("merry_usdc_btoken_balance 2: {}", merry_usdc_btoken_balance);

    // println!("in contract");
    // let usdc_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // println!("usdc_d_rate: {:?}", usdc_data.d_rate);
    // println!("usdc_b_rate: {:?}", usdc_data.b_rate);
    // println!("usdc d supply: {:?}", usdc_data.d_supply);
    // println!("usdc b supply: {:?}", usdc_data.b_supply);
    // println!("backstop credit: {:?}", usdc_data.backstop_credit);
    // fixture.jump(60 * 60 * 24 * 700);

    // let amount = 1 * 10i128.pow(6);
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 3,
    //             address: usdc.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // pool_usdc_balance += amount;
    // merry_usdc_balance -= amount;

    // merry_usdc_btoken_balance += amount
    //     .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
    //     .unwrap();
    // println!("merry_usdc_btoken_balance 3: {}", merry_usdc_btoken_balance);

    // println!("in contract");
    // let usdc_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // println!("usdc_d_rate: {:?}", usdc_data.d_rate);
    // println!("usdc_b_rate: {:?}", usdc_data.b_rate);
    // println!("usdc d supply: {:?}", usdc_data.d_supply);
    // println!("usdc b supply: {:?}", usdc_data.b_supply);
    // println!("backstop credit: {:?}", usdc_data.backstop_credit);
    // fixture.jump(60 * 60 * 24 * 700);

    // let amount = 1 * 10i128.pow(6);
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 3,
    //             address: usdc.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // pool_usdc_balance += amount;
    // merry_usdc_balance -= amount;
    // assert_eq!(usdc.balance(&merry), merry_usdc_balance);
    // assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    // merry_usdc_btoken_balance += amount
    //     .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
    //     .unwrap();
    // // assert_approx_eq_abs(
    // //     result.collateral.get_unchecked(usdc_pool_index),
    // //     merry_usdc_btoken_balance,
    // //     10,
    // // );
    // println!("merry_usdc_btoken_balance 4: {}", merry_usdc_btoken_balance);

    // println!("in contract");
    // let usdc_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // println!("usdc_d_rate: {:?}", usdc_data.d_rate);
    // println!("usdc_b_rate: {:?}", usdc_data.b_rate);
    // println!("usdc d supply: {:?}", usdc_data.d_supply);
    // println!("usdc b supply: {:?}", usdc_data.b_supply);
    // println!("backstop credit: {:?}", usdc_data.backstop_credit);

    // Claim 3 day emissions

    // Claim frodo's three day pool emissions
    // let claim_amount = pool_fixture
    //     .pool
    //     .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    // frodo_blnd_balance += claim_amount;
    // assert_eq!(claim_amount, 4665_6384000);
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND].balance(&frodo),
    //     frodo_blnd_balance
    // );

    // Claim sam's three day pool emissions
    // let mut sam_blnd_balance = 0;
    // let claim_amount = pool_fixture
    //     .pool
    //     .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    // sam_blnd_balance += claim_amount;
    // assert_eq!(claim_amount, 730943066650);
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND].balance(&sam),
    //     sam_blnd_balance
    // );

    // Sam repays some of his STABLE loan
    // let amount = 55_000 * 10i128.pow(6);
    // let result = pool_fixture.pool.submit(
    //     &sam,
    //     &sam,
    //     &sam,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 5,
    //             address: usdc.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // pool_usdc_balance += amount;
    // sam_usdc_balance -= amount;
    // assert_eq!(usdc.balance(&sam), sam_usdc_balance);
    // assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    // sam_usdc_dtoken_balance -= amount
    //     .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.liabilities.get_unchecked(usdc_pool_index),
    //     sam_usdc_dtoken_balance,
    //     10,
    // );

    // // Merry repays some of his XLM loan
    // let amount = 575_000 * SCALAR_7;
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 5,
    //             address: xlm.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    // pool_xlm_balance += amount;
    // merry_xlm_balance -= amount;
    // assert_eq!(xlm.balance(&merry), merry_xlm_balance);
    // assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    // merry_xlm_dtoken_balance -= amount
    //     .fixed_div_floor(reserve_data.d_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.liabilities.get_unchecked(xlm_pool_index),
    //     merry_xlm_dtoken_balance,
    //     10,
    // );

    // // Sam withdraws some of his XLM
    // let amount = 1_000_000 * SCALAR_7;
    // let result = pool_fixture.pool.submit(
    //     &sam,
    //     &sam,
    //     &sam,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 3,
    //             address: xlm.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    // pool_xlm_balance -= amount;
    // sam_xlm_balance += amount;
    // assert_eq!(xlm.balance(&sam), sam_xlm_balance);
    // assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    // sam_xlm_btoken_balance -= amount
    //     .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.collateral.get_unchecked(xlm_pool_index),
    //     sam_xlm_btoken_balance,
    //     10,
    // );

    // // Merry withdraws some of his STABLE
    // let amount = 100_000 * 10i128.pow(6);
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 3,
    //             address: usdc.address.clone(),
    //             amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // pool_usdc_balance -= amount;
    // merry_usdc_balance += amount;
    // assert_eq!(usdc.balance(&merry), merry_usdc_balance);
    // assert_eq!(usdc.balance(&pool_fixture.pool.address), pool_usdc_balance);
    // merry_usdc_btoken_balance -= amount
    //     .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
    //     .unwrap();
    // assert_approx_eq_abs(
    //     result.collateral.get_unchecked(usdc_pool_index),
    //     merry_usdc_btoken_balance,
    //     10,
    // );

    // // Let one month pass
    // fixture.jump(60 * 60 * 24 * 30);

    // // Distribute emissions
    // fixture.emitter.distribute();
    // fixture.backstop.update_emission_cycle();
    // pool_fixture.pool.update_emissions();

    // // Frodo claim emissions
    // // let claim_amount = pool_fixture
    // //     .pool
    // //     .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    // // frodo_blnd_balance += claim_amount;
    // // assert_eq!(claim_amount, 116731656000);
    // // assert_eq!(
    // //     fixture.tokens[TokenIndex::BLND].balance(&frodo),
    // //     frodo_blnd_balance
    // // );
    // // fixture.backstop.claim(
    // //     &frodo,
    // //     &vec![&fixture.env, pool_fixture.pool.address.clone()],
    // //     &frodo,
    // // );
    // // frodo_blnd_balance += 420798_0000000;
    // // assert_eq!(
    // //     fixture.tokens[TokenIndex::BLND].balance(&frodo),
    // //     frodo_blnd_balance
    // // );

    // // Sam claim emissions
    // // let claim_amount = pool_fixture
    // //     .pool
    // //     .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    // // sam_blnd_balance += claim_amount;
    // // assert_eq!(claim_amount, 90908_8243315);
    // // assert_eq!(
    // //     fixture.tokens[TokenIndex::BLND].balance(&sam),
    // //     sam_blnd_balance
    // // );

    // // Let a year go by and call update every week
    // for _ in 0..52 {
    //     // Let one week pass
    //     fixture.jump(60 * 60 * 24 * 7);
    //     // Update emissions
    //     fixture.emitter.distribute();
    //     fixture.backstop.update_emission_cycle();
    //     pool_fixture.pool.update_emissions();
    // }

    // // Frodo claims a year worth of backstop emissions
    // fixture.backstop.claim(
    //     &frodo,
    //     &vec![&fixture.env, pool_fixture.pool.address.clone()],
    //     &frodo,
    // );
    // // frodo_blnd_balance += 22014720_0000000;
    // // assert_eq!(
    // //     fixture.tokens[TokenIndex::BLND].balance(&frodo),
    // //     frodo_blnd_balance
    // // );

    // // Frodo claims a year worth of pool emissions
    // let claim_amount = pool_fixture
    //     .pool
    //     .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
    // frodo_blnd_balance += claim_amount;
    // assert_eq!(claim_amount, 1073627_6928000);
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND].balance(&frodo),
    //     frodo_blnd_balance
    // );

    // // Sam claims a year worth of pool emissions
    // let claim_amount = pool_fixture
    //     .pool
    //     .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
    // sam_blnd_balance += claim_amount;
    // assert_eq!(claim_amount, 8361247_4076689);
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND].balance(&sam),
    //     sam_blnd_balance
    // );

    // // Sam repays his STABLE loan
    // let amount = sam_usdc_dtoken_balance
    //     .fixed_mul_ceil(1_100_000_000, SCALAR_9)
    //     .unwrap();
    // let result = pool_fixture.pool.submit(
    //     &sam,
    //     &sam,
    //     &sam,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 5,
    //             address: usdc.address.clone(),
    //             amount: amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // let est_amount = sam_usdc_dtoken_balance
    //     .fixed_mul_ceil(reserve_data.d_rate, SCALAR_9)
    //     .unwrap();
    // pool_usdc_balance += est_amount;
    // sam_usdc_balance -= est_amount;
    // assert_approx_eq_abs(usdc.balance(&sam), sam_usdc_balance, 100);
    // assert_approx_eq_abs(
    //     usdc.balance(&pool_fixture.pool.address),
    //     pool_usdc_balance,
    //     100,
    // );
    // assert_eq!(result.liabilities.get(usdc_pool_index), None);
    // assert_eq!(result.liabilities.len(), 0);

    // // Merry repays his XLM loan
    // let amount = merry_xlm_dtoken_balance
    //     .fixed_mul_ceil(1_250_000_000, SCALAR_9)
    //     .unwrap();
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 5,
    //             address: xlm.address.clone(),
    //             amount: amount,
    //         },
    //     ],
    // );
    // let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    // let est_amount = merry_xlm_dtoken_balance
    //     .fixed_mul_ceil(reserve_data.d_rate, SCALAR_9)
    //     .unwrap();
    // pool_xlm_balance += est_amount;
    // merry_xlm_balance -= est_amount;
    // assert_approx_eq_abs(xlm.balance(&merry), merry_xlm_balance, 100);
    // assert_approx_eq_abs(
    //     xlm.balance(&pool_fixture.pool.address),
    //     pool_xlm_balance,
    //     100,
    // );
    // assert_eq!(result.liabilities.get(xlm_pool_index), None);
    // assert_eq!(result.liabilities.len(), 0);

    // // Sam withdraws all of his XLM
    // let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);
    // let amount = sam_xlm_btoken_balance
    //     .fixed_mul_ceil(reserve_data.b_rate, SCALAR_9)
    //     .unwrap();
    // let result = pool_fixture.pool.submit(
    //     &sam,
    //     &sam,
    //     &sam,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 3,
    //             address: xlm.address.clone(),
    //             amount: amount,
    //         },
    //     ],
    // );
    // pool_xlm_balance -= amount;
    // sam_xlm_balance += amount;
    // assert_approx_eq_abs(xlm.balance(&sam), sam_xlm_balance, 10);
    // assert_approx_eq_abs(
    //     xlm.balance(&pool_fixture.pool.address),
    //     pool_xlm_balance,
    //     10,
    // );
    // assert_eq!(result.collateral.get(xlm_pool_index), None);

    // // Merry withdraws all of his STABLE
    // let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);
    // let amount = merry_usdc_btoken_balance
    //     .fixed_mul_ceil(reserve_data.b_rate, SCALAR_9)
    //     .unwrap();
    // let result = pool_fixture.pool.submit(
    //     &merry,
    //     &merry,
    //     &merry,
    //     &vec![
    //         &fixture.env,
    //         Request {
    //             request_type: 3,
    //             address: usdc.address.clone(),
    //             amount: amount,
    //         },
    //     ],
    // );
    // pool_usdc_balance -= amount;
    // merry_usdc_balance += amount;
    // assert_approx_eq_abs(usdc.balance(&merry), merry_usdc_balance, 10);
    // assert_approx_eq_abs(
    //     usdc.balance(&pool_fixture.pool.address),
    //     pool_usdc_balance,
    //     10,
    // );
    // assert_eq!(result.collateral.get(usdc_pool_index), None);

    // // Frodo queues for withdrawal a portion of his backstop deposit
    // // Backstop shares are still 1 to 1 with BSTOP tokens - no donation via auction or other means has occurred
    // let mut frodo_bstop_token_balance = fixture.lp.balance(&frodo);
    // let mut backstop_bstop_token_balance = fixture.lp.balance(&fixture.backstop.address);
    // let amount = 500 * SCALAR_7;
    // let result = fixture
    //     .backstop
    //     .queue_withdrawal(&frodo, &pool_fixture.pool.address, &amount);
    // assert_eq!(result.amount, amount);
    // assert_eq!(
    //     result.exp,
    //     fixture.env.ledger().timestamp() + 60 * 60 * 24 * 30
    // );
    // assert_eq!(fixture.lp.balance(&frodo), frodo_bstop_token_balance);
    // assert_eq!(
    //     fixture.lp.balance(&fixture.backstop.address),
    //     backstop_bstop_token_balance
    // );

    // // Time passes and Frodo withdraws his queued for withdrawal backstop deposit
    // fixture.jump(60 * 60 * 24 * 30 + 1);
    // let result = fixture
    //     .backstop
    //     .withdraw(&frodo, &pool_fixture.pool.address, &amount);
    // frodo_bstop_token_balance += result;
    // backstop_bstop_token_balance -= result;
    // assert_eq!(result, amount);
    // assert_eq!(fixture.lp.balance(&frodo), frodo_bstop_token_balance);
    // assert_eq!(
    //     fixture.lp.balance(&fixture.backstop.address),
    //     backstop_bstop_token_balance
    // );
}
