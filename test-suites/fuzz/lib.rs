//! Common code for fuzzing test suites.
#![allow(unused)]
#![no_main]

use soroban_fixed_point_math::FixedPoint;
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
pub struct NatI128(
    #[arbitrary(with = |u: &mut Unstructured| u.int_in_range(0..=i128::MAX))] pub i128,
);

type ContractResult<T> = Result<T, Result<soroban_sdk::Error, core::convert::Infallible>>;

/// Panic if a contract call result might have been the result of an unexpected panic.
///
/// Calls that return an error with type `ScErrorType::WasmVm` and code `ScErrorCode::InvalidAction`
/// are assumed to be unintended errors. These are the codes that result from plain `panic!` invocations,
/// thus contracts should never simply call `panic!`, but instead use `panic_with_error!`.
///
/// Other rare types of internal exception can return `InvalidAction`.
#[track_caller]
pub fn verify_contract_result<T>(env: &soroban_sdk::Env, r: &ContractResult<T>) {
    use soroban_sdk::testutils::Events;
    use soroban_sdk::xdr::{ScErrorCode, ScErrorType};
    use soroban_sdk::{ConversionError, Error};
    match r {
        Err(Ok(e)) => {
            if e.is_type(ScErrorType::WasmVm) && e.is_code(ScErrorCode::InvalidAction) {
                let msg = "contract failed with InvalidAction - unexpected panic?";
                eprintln!("{msg}");
                eprintln!("recent events (10):");
                for (i, event) in env.events().all().iter().rev().take(10).enumerate() {
                    eprintln!("{i}: {event:?}");
                }
                panic!("{msg}");
            }
        }
        _ => {}
    }
}

/// The set of tokens that the pool supports.
#[derive(Arbitrary, Debug, Clone, Copy)]
pub enum PoolReserveToken {
    WETH = 2,
    XLM = 3,
    STABLE = 4,
}

/// Jump the Env `timestamp` forward by `amount` seconds.
///
/// Does not change the `sequence` to prevent any ledger expirations.
#[derive(Arbitrary, Debug)]
pub struct PassTime {
    pub amount: u64,
}

/// Jump the Env `timestamp` forward by `amount` seconds and the Env
/// `sequence` by `amount` / 5 blocks.
#[derive(Arbitrary, Debug)]
pub struct PassTimeAndBlocks {
    pub amount: u64,
}

/// Supply `amount` of `token` into the pool.
#[derive(Arbitrary, Debug)]
pub struct Supply {
    pub amount: i128,
    pub token: PoolReserveToken,
}

/// Withdraw `amount` of `token` out of the pool for `user`.
#[derive(Arbitrary, Debug)]
pub struct Withdraw {
    pub amount: i128,
    pub token: PoolReserveToken,
}

/// Borrow `amount` of `token` out of the pool for `user`.
#[derive(Arbitrary, Debug)]
pub struct Borrow {
    pub amount: i128,
    pub token: PoolReserveToken,
}

/// Repay `amount` of `token` into the pool for `user`.
#[derive(Arbitrary, Debug)]
pub struct Repay {
    pub amount: i128,
    pub token: PoolReserveToken,
}

/// Claim emissions from the pool for `user`.
#[derive(Arbitrary, Debug)]
pub struct ClaimPool {}

/// Claim emissions from the backstop for `user`.
#[derive(Arbitrary, Debug)]
pub struct ClaimBackstop {}

impl PassTime {
    pub fn run(&self, fixture: &TestFixture) {
        fixture.jump(self.amount);
    }
}

impl PassTimeAndBlocks {
    pub fn run(&self, fixture: &TestFixture) {
        fixture.jump_with_sequence(self.amount);
    }
}

impl Supply {
    pub fn run(&self, fixture: &TestFixture, user_index: usize) {
        let pool_fixture = fixture.pools.get(0).unwrap();
        let token = fixture
            .tokens
            .get(self.token as usize)
            .unwrap()
            .address
            .clone();
        let user = fixture.users.get(user_index).unwrap();
        let r = pool_fixture.pool.try_submit(
            &user,
            &user,
            &user,
            &vec![
                &fixture.env,
                Request {
                    request_type: 2,
                    address: token,
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&fixture.env, &r);
    }
}

impl Withdraw {
    pub fn run(&self, fixture: &TestFixture, user_index: usize) {
        let pool_fixture = fixture.pools.get(0).unwrap();
        let token = fixture
            .tokens
            .get(self.token as usize)
            .unwrap()
            .address
            .clone();
        let user = fixture.users.get(user_index).unwrap();
        let r = pool_fixture.pool.try_submit(
            &user,
            &user,
            &user,
            &vec![
                &fixture.env,
                Request {
                    request_type: 3,
                    address: token,
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&fixture.env, &r);
    }
}

impl Borrow {
    pub fn run(&self, fixture: &TestFixture, user_index: usize) {
        let pool_fixture = fixture.pools.get(0).unwrap();
        let token = fixture
            .tokens
            .get(self.token as usize)
            .unwrap()
            .address
            .clone();
        let user = fixture.users.get(user_index).unwrap();
        let r = pool_fixture.pool.try_submit(
            &user,
            &user,
            &user,
            &vec![
                &fixture.env,
                Request {
                    request_type: 4,
                    address: token,
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&fixture.env, &r);
    }
}

impl Repay {
    pub fn run(&self, fixture: &TestFixture, user_index: usize) {
        let pool_fixture = fixture.pools.get(0).unwrap();
        let token = fixture
            .tokens
            .get(self.token as usize)
            .unwrap()
            .address
            .clone();
        let user = fixture.users.get(user_index).unwrap();
        let r = pool_fixture.pool.try_submit(
            &user,
            &user,
            &user,
            &vec![
                &fixture.env,
                Request {
                    request_type: 5,
                    address: token,
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&fixture.env, &r);
    }
}

impl ClaimPool {
    pub fn run(&self, fixture: &TestFixture, user_index: usize) {
        let pool_fixture = fixture.pools.get(0).unwrap();
        let user = fixture.users.get(user_index).unwrap();
        let r = pool_fixture
            .pool
            .try_claim(&user, &vec![&fixture.env, 0, 3], &user);
        verify_contract_result(&fixture.env, &r);
    }
}

impl ClaimBackstop {
    pub fn run(&self, fixture: &TestFixture, user_index: usize) {
        let pool_fixture = fixture.pools.get(0).unwrap();
        let user = fixture.users.get(user_index).unwrap();
        let r = fixture.backstop.try_claim(
            &user,
            &vec![&fixture.env, pool_fixture.pool.address.clone()],
            &user,
        );
        verify_contract_result(&fixture.env, &r);
    }
}
