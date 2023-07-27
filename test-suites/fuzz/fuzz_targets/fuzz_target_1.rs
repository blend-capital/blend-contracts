#![allow(unused)]
#![no_main]

use libfuzzer_sys::fuzz_target;
use fixed_point_math::FixedPoint;
use lending_pool::Request;
use soroban_sdk::{testutils::Address as _, vec, Address};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7, SCALAR_9, TestFixture},
};
use soroban_sdk::arbitrary::arbitrary::{self, Arbitrary, Unstructured};

#[derive(Arbitrary, Debug)]
struct Input {
    sam_usdc_balance: NatI128,
    sam_xlm_balance: NatI128,
    merry_usdc_balance: NatI128,
    merry_xlm_balance: NatI128,
    merry_supply_usdc_amount: i128, //NatI128,
    sam_supply_xlm_amount: i128, //NatI128,
    sam_borrow_usdc_amount: i128, //NatI128,
    merry_borrow_xlm_amount: i128, //NatI128,
    sam_repay_usdc_amount: i128,
    merry_repay_xlm_amount: i128,
    sam_withdraw_xlm_amount: i128,
    merry_withdraw_usdc_amount: i128,
}

#[derive(Arbitrary, Debug)]
struct NatI128(
    #[arbitrary(with = |u: &mut Unstructured| u.int_in_range(0..=i128::MAX))]
    pub i128,
);

fuzz_target!(|input: Input| {
    let (fixture, frodo) = create_fixture_with_data(false);
    let pool_fixture = &fixture.pools[0];
    let usdc_pool_index = pool_fixture.reserves[&TokenIndex::USDC];
    let xlm_pool_index = pool_fixture.reserves[&TokenIndex::XLM];

    // Create two new users
    let sam = Address::random(&fixture.env); // sam will be supplying XLM and borrowing USDC
    let merry = Address::random(&fixture.env); // merry will be supplying USDC and borrowing XLM

    // Mint users tokens
    let usdc = &fixture.tokens[TokenIndex::USDC];
    let xlm = &fixture.tokens[TokenIndex::XLM];
    let mut sam_usdc_balance = input.sam_usdc_balance.0;
    let mut sam_xlm_balance = input.sam_xlm_balance.0;
    let mut merry_usdc_balance = input.merry_usdc_balance.0;
    let mut merry_xlm_balance = input.merry_xlm_balance.0;
    usdc.mint(&sam, &input.sam_usdc_balance.0);
    usdc.mint(&merry, &input.merry_usdc_balance.0);
    xlm.mint(&sam, &input.sam_xlm_balance.0);
    xlm.mint(&merry, &input.merry_xlm_balance.0);

    let mut pool_usdc_balance = usdc.balance(&pool_fixture.pool.address);
    let mut pool_xlm_balance = xlm.balance(&pool_fixture.pool.address);

    let mut sam_xlm_btoken_balance = 0;
    let mut sam_usdc_dtoken_balance = 0;
    let mut merry_usdc_btoken_balance = 0;
    let mut merry_xlm_dtoken_balance = 0;

    {
        // Merry supply USDC
        let amount = input.merry_supply_usdc_amount;
        let result = pool_fixture.pool.try_submit(
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

        let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);

        if let Ok(Ok(result)) = result {
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
        }

        fixture.assert_invariants();
    }

    {
        // Sam supply XLM
        let amount = input.sam_supply_xlm_amount;
        let result = pool_fixture.pool.try_submit(
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

        let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);

        if let Ok(Ok(result)) = result {
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
        }

        fixture.assert_invariants();
    }

    {
        // Sam borrow USDC
        let amount = input.sam_borrow_usdc_amount;
        let result = pool_fixture.pool.try_submit(
            &sam,
            &sam,
            &sam,
            &vec![
                &fixture.env,
                Request {
                    request_type: 4,
                    address: usdc.address.clone(),
                    amount,
                },
            ],
        );

        // todo assert sam max borrow
        // Sam max borrow is .75*.95*.1*1_900_000 = 135_375 USDC

        let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);

        if let Ok(Ok(result)) = result {
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
        }

        fixture.assert_invariants();
    }

    {
        // Merry borrow XLM
        let amount = input.merry_borrow_xlm_amount;
        let result = pool_fixture.pool.try_submit(
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

        // todo assert merry max borrow
        // Merry max borrow is .75*.9*190_000/.1 = 1_282_5000 XLM

        let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);

        if let Ok(Ok(result)) = result {
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
        }

        fixture.assert_invariants();
    }

    {
        // todo fix assertions
        // claim frodo's setup emissions (1h1m passes during setup)
        // - Frodo should receive 60 * 61 * .3 = 1098 BLND from the pool claim
        // - Frodo should receive 60 * 61 * .7 = 2562 BLND from the backstop claim
        let mut frodo_blnd_balance = 0;
        let claim_amount = pool_fixture
            .pool
            .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
        frodo_blnd_balance += claim_amount;
        // todo assert_eq!(claim_amount, 1098_0000000);
        assert_eq!(
            fixture.tokens[TokenIndex::BLND].balance(&frodo),
            frodo_blnd_balance
        );
        fixture.backstop.claim(
            &frodo,
            &vec![&fixture.env, pool_fixture.pool.address.clone()],
            &frodo,
        );
        // frodo_blnd_balance += 2562_0000000;
        //assert_eq!(
        //    fixture.tokens[TokenIndex::BLND].balance(&frodo),
        //    frodo_blnd_balance
        //);

        fixture.assert_invariants();
    }

    // Let three days pass
    fixture.jump(60 * 60 * 24 * 3);

    {
        // todo fix assertions
        // Claim frodo's three day pool emissions
        let claim_amount = pool_fixture
            .pool
            .claim(&frodo, &vec![&fixture.env, 0, 3], &frodo);
        //frodo_blnd_balance += claim_amount;
        //assert_eq!(claim_amount, 4665_6384000);
        //assert_eq!(
        //    fixture.tokens[TokenIndex::BLND].balance(&frodo),
        //    frodo_blnd_balance
        //);

        fixture.assert_invariants();
    }

    {
        // todo fix assertions
        // Claim sam's three day pool emissions
        let mut sam_blnd_balance = 0;
        let claim_amount = pool_fixture
            .pool
            .claim(&sam, &vec![&fixture.env, 0, 3], &sam);
        sam_blnd_balance += claim_amount;
        //assert_eq!(claim_amount, 730943066650);
        //assert_eq!(
        //    fixture.tokens[TokenIndex::BLND].balance(&sam),
        //    sam_blnd_balance
        //);

        fixture.assert_invariants();
    }

    {
        // Sam repays some of his USDC loan
        let amount = input.sam_repay_usdc_amount;
        let result = pool_fixture.pool.try_submit(
            &sam,
            &sam,
            &sam,
            &vec![
                &fixture.env,
                Request {
                    request_type: 5,
                    address: usdc.address.clone(),
                    amount,
                },
            ],
        );

        let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);

        // fixme below assertions don't always hold
        pool_usdc_balance = usdc.balance(&pool_fixture.pool.address);
        sam_usdc_balance = usdc.balance(&merry);
        /*if let Ok(Ok(result)) = result {
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
        }*/

        fixture.assert_invariants();
    }

    {
        // Merry repays some of his XLM loan
        let amount = input.merry_repay_xlm_amount;
        let result = pool_fixture.pool.try_submit(
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

        let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);

        // fixme below assertions don't always hold
        pool_xlm_balance = xlm.balance(&pool_fixture.pool.address);
        merry_xlm_balance = xlm.balance(&merry);
        /*if let Ok(Ok(result)) = result {
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
        }*/

        fixture.assert_invariants();
    }

    {
        // Sam withdraws some of his XLM
        let amount = input.sam_withdraw_xlm_amount;
        let result = pool_fixture.pool.try_submit(
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

        let reserve_data = pool_fixture.pool.get_reserve_data(&xlm.address);

        // fixme below assertions don't always hold
        pool_xlm_balance = xlm.balance(&pool_fixture.pool.address);
        sam_xlm_balance = xlm.balance(&sam);
        /*if let Ok(Ok(result)) = result {
            pool_xlm_balance -= amount;
            sam_xlm_balance += amount;
            sam_xlm_balance = xlm.balance(&sam);
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
        }*/

        fixture.assert_invariants();
    }

    {
        // Merry withdraws some of his USDC
        let amount = input.merry_withdraw_usdc_amount;
        let result = pool_fixture.pool.try_submit(
            &merry,
            &merry,
            &merry,
            &vec![
                &fixture.env,
                Request {
                    request_type: 3,
                    address: usdc.address.clone(),
                    amount,
                },
            ],
        );

        let reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);

        // fixme below assertions don't always hold
        pool_usdc_balance = usdc.balance(&pool_fixture.pool.address);
        merry_usdc_balance = usdc.balance(&sam);
        /*if let Ok(Ok(result)) = result {
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
        }*/

        fixture.assert_invariants();
    }

    // todo
});

#[extension_trait::extension_trait]
impl Asserts for TestFixture<'_> {
    fn assert_invariants(&self) {
        let pool_fixture = &self.pools[0];
        let usdc = &self.tokens[TokenIndex::USDC];
        let usdc_pool_index = pool_fixture.reserves[&TokenIndex::USDC];

        let usdc_reserve_config = pool_fixture.pool.get_reserve_config(&usdc.address);
        let usdc_reserve_data = pool_fixture.pool.get_reserve_data(&usdc.address);

        //eprintln!("{:#?}", usdc_reserve_config);
        //eprintln!("{:#?}", usdc_reserve_data);

        // todo
    }
}
