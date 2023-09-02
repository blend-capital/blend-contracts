#![allow(unused)]
#![no_main]

use libfuzzer_sys::fuzz_target;
use fixed_point_math::FixedPoint;
use lending_pool::{Request, PoolState, PositionData};
use soroban_sdk::{testutils::Address as _, vec, Address};
use test_suites::{
    token::{TokenClient},
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7, SCALAR_9, TestFixture, PoolFixture},
};
use soroban_sdk::arbitrary::arbitrary::{self, Arbitrary, Unstructured};

#[derive(Arbitrary, Debug)]
struct Input {
    sam_xlm_balance: NatI128,
    sam_weth_balance: NatI128,
    merry_xlm_balance: NatI128,
    merry_weth_balance: NatI128,
    commands: [Command; 10],
}

#[derive(Arbitrary, Debug)]
struct NatI128(
    #[arbitrary(with = |u: &mut Unstructured| u.int_in_range(0..=i128::MAX))]
    pub i128,
);

#[derive(Arbitrary, Debug)]
enum Command {
    PassTime(PassTime),

    MerrySupplyXlm(MerrySupplyXlm),
    SamSupplyWeth(SamSupplyWeth),
    MerryWithdrawXlm(MerryWithdrawXlm),
    SamWithdrawWeth(SamWithdrawWeth),

    MerryBorrowWeth(MerryBorrowWeth),
    SamBorrowXlm(SamBorrowXlm),
    MerryRepayWeth(MerryRepayWeth),
    SamRepayXlm(SamRepayXlm),

    FrodoClaimPool(FrodoClaimPool),
    FrodoClaimBackstop(FrodoClaimBackstop),
    MerryClaimPool(MerryClaimPool),
    SamClaimPool(SamClaimPool),
}

#[derive(Arbitrary, Debug)]
struct PassTime {
    amount: u64,
}

#[derive(Arbitrary, Debug)]
struct MerrySupplyXlm {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct SamSupplyWeth {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct MerryWithdrawXlm {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct SamWithdrawWeth {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct MerryBorrowWeth {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct SamBorrowXlm {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct MerryRepayWeth {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct SamRepayXlm {
    amount: i128,
}

#[derive(Arbitrary, Debug)]
struct FrodoClaimPool;

#[derive(Arbitrary, Debug)]
struct FrodoClaimBackstop;

#[derive(Arbitrary, Debug)]
struct MerryClaimPool;

#[derive(Arbitrary, Debug)]
struct SamClaimPool;

struct State<'a> {
    fixture: &'a TestFixture<'a>,
    pool_fixture: &'a PoolFixture<'a>,
    frodo: Address,
    sam: Address,
    merry: Address,
    xlm: &'a TokenClient<'a>,
    weth: &'a TokenClient<'a>,
}

fuzz_target!(|input: Input| {
    let (fixture, frodo) = create_fixture_with_data(false);
    let pool_fixture = &fixture.pools[0];
    let xlm_pool_index = pool_fixture.reserves[&TokenIndex::XLM];
    let weth_pool_index = pool_fixture.reserves[&TokenIndex::WETH];

    // Create two new users
    let sam = Address::random(&fixture.env); // sam will be supplying WETH and borrowing XLM
    let merry = Address::random(&fixture.env); // merry will be supplying XLM and borrowing WETH

    // Mint users tokens
    let xlm = &fixture.tokens[TokenIndex::XLM];
    let weth = &fixture.tokens[TokenIndex::WETH];

    let mut sam_xlm_balance = input.sam_xlm_balance.0;
    let mut sam_weth_balance = input.sam_weth_balance.0;
    let mut merry_xlm_balance = input.merry_xlm_balance.0;
    let mut merry_weth_balance = input.merry_weth_balance.0;
    xlm.mint(&sam, &input.sam_xlm_balance.0);
    xlm.mint(&merry, &input.merry_xlm_balance.0);
    weth.mint(&sam, &input.sam_weth_balance.0);
    weth.mint(&merry, &input.merry_weth_balance.0);

    let state = State {
        fixture: &fixture,
        pool_fixture,
        frodo,
        sam,
        merry,
        xlm,
        weth,
    };

    for command in &input.commands {
        command.run(&state);
        fixture.assert_invariants();
    }
});

type ContractResult<T> = Result<Result<T, soroban_sdk::ConversionError>, Result<soroban_sdk::Error, core::convert::Infallible>>;

/// Panic if a contract call result might have been the result of an unexpected panic.
///
/// Calls that return an error with type `ScErrorType::WasmVm` and code `ScErrorCode::InvalidAction`
/// are assumed to be unintended errors. These are the codes that result from plain `panic!` invocations,
/// thus contracts should never simply call `panic!`, but instead use `panic_with_error!`.
///
/// Other rare types of internal exception can return `InvalidAction`.
#[track_caller]
fn verify_contract_result<T>(env: &soroban_sdk::Env, r: &ContractResult<T>) {
    use soroban_sdk::{Error, ConversionError};
    use soroban_sdk::xdr::{ScErrorType, ScErrorCode};
    use soroban_sdk::testutils::Events;
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
        _ => { }
    }
}

impl Command {
    fn run(&self, state: &State) {
        use Command::*;
        match self {
            PassTime(cmd) => cmd.run(state),
            MerrySupplyXlm(cmd) => cmd.run(state),
            MerryWithdrawXlm(cmd) => cmd.run(state),
            SamSupplyWeth(cmd) => cmd.run(state),
            SamWithdrawWeth(cmd) => cmd.run(state),
            MerryBorrowWeth(cmd) => cmd.run(state),
            MerryRepayWeth(cmd) => cmd.run(state),
            SamBorrowXlm(cmd) => cmd.run(state),
            SamRepayXlm(cmd) => cmd.run(state),
            FrodoClaimPool(cmd) => cmd.run(state),
            FrodoClaimBackstop(cmd) => cmd.run(state),
            MerryClaimPool(cmd) => cmd.run(state),
            SamClaimPool(cmd) => cmd.run(state),
        }
    }
}

impl PassTime {
    fn run(&self, state: &State) {
        state.fixture.jump(self.amount);
    }
}

impl MerrySupplyXlm {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.merry,
            &state.merry,
            &state.merry,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 2,
                    address: state.xlm.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl SamSupplyWeth {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.sam,
            &state.sam,
            &state.sam,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 2,
                    address: state.weth.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl MerryWithdrawXlm {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.merry,
            &state.merry,
            &state.merry,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 3,
                    address: state.xlm.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl SamWithdrawWeth {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.sam,
            &state.sam,
            &state.sam,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 3,
                    address: state.weth.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl MerryBorrowWeth {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.merry,
            &state.merry,
            &state.merry,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 4,
                    address: state.weth.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl SamBorrowXlm {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.sam,
            &state.sam,
            &state.sam,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 4,
                    address: state.xlm.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl MerryRepayWeth {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.merry,
            &state.merry,
            &state.merry,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 5,
                    address: state.weth.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl SamRepayXlm {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_submit(
            &state.sam,
            &state.sam,
            &state.sam,
            &vec![
                &state.fixture.env,
                Request {
                    request_type: 5,
                    address: state.xlm.address.clone(),
                    amount: self.amount,
                },
            ],
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl FrodoClaimPool {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_claim(
            &state.frodo,
            &vec![&state.fixture.env, 0, 3],
            &state.frodo,
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl FrodoClaimBackstop {
    fn run(&self, state: &State) {
        let r = state.fixture.backstop.try_claim(
            &state.frodo,
            &vec![&state.fixture.env, state.pool_fixture.pool.address.clone()],
            &state.frodo,                  
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl MerryClaimPool {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_claim(
            &state.merry,
            &vec![&state.fixture.env, 0, 3],
            &state.merry,
        );
        verify_contract_result(&state.fixture.env, &r);
    }
}

impl SamClaimPool {
    fn run(&self, state: &State) {
        let r = state.pool_fixture.pool.try_claim(
            &state.sam,
            &vec![&state.fixture.env, 0, 3],
            &state.sam,
        );
        verify_contract_result(&state.fixture.env, &r);
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
                supply += asset_to_base.fixed_mul_floor(reserve.total_supply() + reserve.backstop_credit, reserve.scalar).unwrap();
                liabilities += asset_to_base.fixed_mul_ceil(reserve.total_liabilities(), reserve.scalar).unwrap();
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
            let data = PositionData::calculate_from_positions(&self.env, &mut pool_state, &positions);
            assert!(data.as_health_factor() > data.scalar);
        });
    }
}
