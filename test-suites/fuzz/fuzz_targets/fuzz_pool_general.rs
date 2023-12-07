#![allow(unused)]
#![no_main]

use soroban_fixed_point_math::FixedPoint;
use fuzz_common::{
    verify_contract_result, Borrow, ClaimPool, NatI128, PassTime, Repay, Supply, Withdraw,
};
use lending_pool::{PoolState, PositionData, Request};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::arbitrary::arbitrary::{self, Arbitrary, Unstructured};
use soroban_sdk::{testutils::Address as _, vec, Address};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{PoolFixture, TestFixture, TokenIndex, SCALAR_7, SCALAR_9},
    token::TokenClient,
};

#[derive(Arbitrary, Debug)]
struct Input {
    sam_xlm_balance: NatI128,
    sam_weth_balance: NatI128,
    sam_stable_balance: NatI128,
    merry_xlm_balance: NatI128,
    merry_weth_balance: NatI128,
    merry_stable_balance: NatI128,
    commands: [Command; 10],
}

#[derive(Arbitrary, Debug)]
enum Command {
    // Misc
    PassTime(PassTime),

    // Sam (1) Pool Commands
    SamSupply(Supply),
    SamWithdraw(Withdraw),
    SamBorrow(Borrow),
    SamRepay(Repay),
    SamClaimPool(ClaimPool),

    // Sam (2) Pool Commands
    MerrySupply(Supply),
    MerryWithdraw(Withdraw),
    MerryBorrow(Borrow),
    MerryRepay(Repay),
    MerryClaimPool(ClaimPool),
}

fuzz_target!(|input: Input| {
    let mut fixture = create_fixture_with_data(false);

    // Create two new users
    let sam = Address::generate(&fixture.env);
    fixture.users.push(sam.clone());
    let merry = Address::generate(&fixture.env);
    fixture.users.push(merry.clone());

    // Mint users tokens
    let xlm = &fixture.tokens[TokenIndex::XLM];
    let weth = &fixture.tokens[TokenIndex::WETH];
    let stable = &fixture.tokens[TokenIndex::STABLE];

    xlm.mint(&sam, &input.sam_xlm_balance.0);
    xlm.mint(&merry, &input.merry_xlm_balance.0);
    weth.mint(&sam, &input.sam_weth_balance.0);
    weth.mint(&merry, &input.merry_weth_balance.0);
    stable.mint(&sam, &input.sam_stable_balance.0);
    stable.mint(&merry, &input.merry_stable_balance.0);

    for command in &input.commands {
        command.run(&fixture);
        fixture.assert_invariants();
    }
});

impl Command {
    fn run(&self, fixture: &TestFixture) {
        use Command::*;
        match self {
            PassTime(cmd) => cmd.run(fixture),
            SamSupply(cmd) => cmd.run(fixture, 1),
            SamWithdraw(cmd) => cmd.run(fixture, 1),
            SamBorrow(cmd) => cmd.run(fixture, 1),
            SamRepay(cmd) => cmd.run(fixture, 1),
            SamClaimPool(cmd) => cmd.run(fixture, 1),
            MerrySupply(cmd) => cmd.run(fixture, 2),
            MerryWithdraw(cmd) => cmd.run(fixture, 2),
            MerryBorrow(cmd) => cmd.run(fixture, 2),
            MerryRepay(cmd) => cmd.run(fixture, 2),
            MerryClaimPool(cmd) => cmd.run(fixture, 2),
        }
    }
}

#[extension_trait::extension_trait]
impl Asserts for TestFixture<'_> {
    /// Assert the pool has not lent out more funds than it has
    fn assert_invariants(&self) {
        let pool_fixture = &self.pools[0];

        let mut supply: i128 = 0;
        let mut liabilities: i128 = 0;
        self.env.as_contract(&pool_fixture.pool.address, || {
            let mut pool_state = PoolState::load(&self.env);
            for (token_index, reserve_index) in pool_fixture.reserves.iter() {
                let asset = &self.tokens[token_index.clone()];
                let reserve = pool_state.load_reserve(&self.env, &asset.address);
                let asset_to_base = pool_state.load_price(&self.env, &reserve.asset);
                supply += asset_to_base
                    .fixed_mul_floor(
                        reserve.total_supply() + reserve.backstop_credit,
                        reserve.scalar,
                    )
                    .unwrap();
                liabilities += asset_to_base
                    .fixed_mul_ceil(reserve.total_liabilities(), reserve.scalar)
                    .unwrap();
            }
        });

        assert!(supply > liabilities);
    }

    /// Assert the user is not underwater
    fn assert_user_invariants(&self, user: &Address) {
        let pool_fixture = &self.pools[0];

        let positions = pool_fixture.pool.get_positions(&user);
        self.env.as_contract(&pool_fixture.pool.address, || {
            let mut pool_state = PoolState::load(&self.env);
            let data =
                PositionData::calculate_from_positions(&self.env, &mut pool_state, &positions);
            assert!(data.as_health_factor() > data.scalar);
        });
    }
}
