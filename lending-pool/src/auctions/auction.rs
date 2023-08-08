use crate::{
    constants::SCALAR_7,
    errors::PoolError,
    pool::{Pool, PositionData, User},
    storage,
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{
    contracttype, map, panic_with_error, unwrap::UnwrapOptimized, Address, Env, Map,
};

use super::{
    backstop_interest_auction::{create_interest_auction_data, fill_interest_auction},
    bad_debt_auction::{create_bad_debt_auction_data, fill_bad_debt_auction},
    user_liquidation_auction::{create_user_liq_auction_data, fill_user_liq_auction},
};

#[derive(Clone, PartialEq)]
#[repr(u32)]
pub enum AuctionType {
    UserLiquidation = 0,
    BadDebtAuction = 1,
    InterestAuction = 2,
}

impl AuctionType {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => AuctionType::UserLiquidation,
            1 => AuctionType::BadDebtAuction,
            2 => AuctionType::InterestAuction,
            _ => panic!("internal error"),
        }
    }
}

#[derive(Clone)]
#[contracttype]
pub struct AuctionData {
    pub bid: Map<Address, i128>,
    pub lot: Map<Address, i128>,
    pub block: u32,
}

/// Create an auction. Stores the resulting auction to the ledger to begin on the next block
///
/// Returns the AuctionData object created.
///
/// ### Arguments
/// * `auction_type` - The type of auction being created
///
/// ### Panics
/// If the auction is unable to be created
pub fn create(e: &Env, auction_type: u32) -> AuctionData {
    let backstop = storage::get_backstop(e);
    let auction_data = match AuctionType::from_u32(auction_type) {
        AuctionType::UserLiquidation => {
            panic_with_error!(e, PoolError::BadRequest);
        }
        AuctionType::BadDebtAuction => create_bad_debt_auction_data(e, &backstop),
        AuctionType::InterestAuction => create_interest_auction_data(e, &backstop),
    };

    storage::set_auction(e, &auction_type, &backstop, &auction_data);

    return auction_data;
}

/// Create a liquidation auction. Stores the resulting auction to the ledger to begin on the next block
///
/// Returns the AuctionData object created.
///
/// ### Arguments
/// * `user` - The user being liquidated
/// * `liq_data` - The liquidation metadata
///
/// ### Panics
/// If the auction is unable to be created
pub fn create_liquidation(e: &Env, user: &Address, percent_liquidated: u64) -> AuctionData {
    let auction_data = create_user_liq_auction_data(e, user, percent_liquidated);

    storage::set_auction(
        &e,
        &(AuctionType::UserLiquidation as u32),
        &user,
        &auction_data,
    );

    return auction_data;
}

/// Delete a liquidation auction if the user being liquidated is no longer eligible for liquidation.
///
/// ### Arguments
/// * `auction_type` - The type of auction being created
///
/// ### Panics
/// If no auction exists for the user or if the user is still eligible for liquidation.
pub fn delete_liquidation(e: &Env, user: &Address) {
    if !storage::has_auction(e, &(AuctionType::UserLiquidation as u32), &user) {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let mut pool = Pool::load(e);
    let positions = storage::get_user_positions(e, user);
    let position_data = PositionData::calculate_from_positions(e, &mut pool, &positions);
    position_data.require_healthy(e);
    storage::del_auction(e, &(AuctionType::UserLiquidation as u32), &user);
}

/// Fills the auction from the invoker. The filler is expected to maintain allowances to both
/// the pool and the backstop module.
///
/// TODO: Use auth-next to avoid required allowances
///
/// ### Arguments
/// * `pool` - The pool
/// * `auction_type` - The type of auction to fill
/// * `user` - The user involved in the auction
/// * `filler` - The Address filling the auction
///
/// ### Panics
/// If the auction does not exist, or if the pool is unable to fulfill either side
/// of the auction quote
pub fn fill(
    e: &Env,
    pool: &mut Pool,
    auction_type: u32,
    user: &Address,
    filler_state: &mut User,
    percent_filled: u64,
) {
    let mut auction_data = storage::get_auction(e, &auction_type, user);
    if percent_filled > 1_0000000 || percent_filled == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }
    if percent_filled == 1_0000000 {
        storage::del_auction(e, &auction_type, user);
    } else {
        let new_auction = apply_fill_pct(e, &mut auction_data, percent_filled);
        storage::set_auction(e, &auction_type, user, &new_auction);
    }
    let filled_auction = apply_fill_modifiers(e, &mut auction_data);
    match AuctionType::from_u32(auction_type) {
        AuctionType::UserLiquidation => {
            fill_user_liq_auction(e, pool, &filled_auction, user, filler_state)
        }
        AuctionType::BadDebtAuction => {
            fill_bad_debt_auction(e, pool, &filled_auction, filler_state)
        }
        AuctionType::InterestAuction => {
            fill_interest_auction(e, pool, &filled_auction, &filler_state.address)
        }
    };
}

/// Get the current fill modifiers for the auction
///
/// Returns a tuple of i128's => (bid modifier, lot modifier) scaled
/// to 7 decimal places
pub(super) fn apply_fill_modifiers(e: &Env, auction_data: &mut AuctionData) -> AuctionData {
    let block_dif = i128(e.ledger().sequence() - auction_data.block) * 1_0000000;
    // increment the modifier 0.5% every block
    let per_block_scalar: i128 = 0_0050000;
    if block_dif >= 400_0000000 {
        auction_data.bid = map![&e];
    } else if block_dif > 200_0000000 {
        let bid_mod = 2_0000000
            - block_dif
                .fixed_mul_floor(per_block_scalar, SCALAR_7)
                .unwrap_optimized();
        for (asset, amount) in auction_data.bid.iter() {
            auction_data.bid.set(
                asset,
                amount.fixed_mul_ceil(bid_mod, SCALAR_7).unwrap_optimized(),
            );
        }
    } else if block_dif == 0 {
        auction_data.lot = map![&e];
    } else {
        let lot_mod = block_dif
            .fixed_mul_floor(per_block_scalar, SCALAR_7)
            .unwrap_optimized();
        for (asset, amount) in auction_data.lot.iter() {
            let new_amount = amount.fixed_mul_floor(lot_mod, SCALAR_7).unwrap_optimized();
            // avoid setting the lot to 0
            if new_amount > 0 {
                auction_data.lot.set(asset, new_amount);
            } else {
                auction_data.lot.remove(asset);
            }
        }
    };
    auction_data.clone()
}

fn apply_fill_pct(e: &Env, auction_data: &mut AuctionData, percent_filled: u64) -> AuctionData {
    let mut new_auction_data: AuctionData = AuctionData {
        lot: map![e],
        bid: map![e],
        block: auction_data.block,
    };
    for (asset, amount) in auction_data.bid.iter() {
        // Note: this rounds the amount removed down to the nearest stroop to avoid rounding exploits
        let remaining = amount
            .fixed_mul_floor(1_0000000 - percent_filled as i128, SCALAR_7)
            .unwrap_optimized();
        auction_data.bid.set(asset.clone(), amount - remaining);
        // Avoid setting 0 bids
        if remaining > 0 {
            new_auction_data.bid.set(asset, remaining);
        }
    }
    for (asset, amount) in auction_data.lot.iter() {
        // Note: this rounds the amount removed up to the nearest stroop to avoid rounding exploits
        let remaining = amount
            .fixed_mul_ceil(1_0000000 - percent_filled as i128, SCALAR_7)
            .unwrap_optimized();
        // Avoid setting 0 lots
        if remaining != amount {
            auction_data.lot.set(asset.clone(), amount - remaining);
        } else {
            auction_data.lot.remove(asset.clone());
        }
        new_auction_data.lot.set(asset, remaining);
    }
    return new_auction_data;
}

#[cfg(test)]
mod tests {

    use crate::{pool::Positions, storage::PoolConfig, testutils};

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, Ledger, LedgerInfo},
    };

    #[test]
    fn test_create_bad_debt_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.d_rate = 1_100_000_000;
        reserve_data_0.last_time = 12345;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.d_rate = 1_200_000_000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_data_2.b_rate = 1_100_000_000;
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        backstop_token_client.mint(&samwise, &200_0000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &100_0000000);

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&backstop_token_id, &0_5000000);

        let positions: Positions = Positions {
            collateral: map![&e],
            liabilities: map![
                &e,
                (reserve_config_0.index, 10_0000000),
                (reserve_config_1.index, 2_5000000)
            ],
            supply: map![&e],
        };

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &backstop_address, &positions);

            create(&e, 1);
            assert!(storage::has_auction(&e, &1, &backstop_address));
        });
    }

    #[test]
    fn test_create_interest_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, _) = testutils::create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.last_time = 12345;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_data_2.b_rate = 1_100_000_000;
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);
            let mut reserve_0 = pool.load_reserve(&e, &underlying_0);
            reserve_0.backstop_credit += 100_0000000;
            reserve_0.store(&e);
            let mut reserve_1 = pool.load_reserve(&e, &underlying_1);
            reserve_1.backstop_credit += 25_0000000;
            reserve_1.store(&e);
            create(&e, 2);
            assert!(storage::has_auction(&e, &2, &backstop_address));
        });
    }

    #[test]
    fn test_create_liquidation() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.last_time = 12345;
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_config_0.c_factor = 0_8500000;
        reserve_config_0.l_factor = 0_9000000;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_200_000_000;
        reserve_config_1.c_factor = 0_7500000;
        reserve_config_1.l_factor = 0_7500000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_config_2.c_factor = 0_0000000;
        reserve_config_2.l_factor = 0_7000000;
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        let liq_pct = 4500000;
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            create_liquidation(&e, &samwise, liq_pct);
            assert!(storage::has_auction(&e, &0, &samwise));
        });
    }
    #[test]
    #[should_panic]
    //#[should_panic(expected = "ContractError(2)")]
    fn test_create_user_liquidation_errors() {
        let e = Env::default();
        let pool_id = Address::random(&e);
        let backstop_id = Address::random(&e);

        e.as_contract(&pool_id, || {
            storage::set_backstop(&e, &backstop_id);

            create(&e, AuctionType::UserLiquidation as u32);
        });
    }

    #[test]
    fn test_delete_user_liquidation() {
        let e = Env::default();
        e.mock_all_auths();
        let pool_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config_0, reserve_data_0) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(
            &e,
            &pool_id,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_id,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);
        oracle_client.set_price(&underlying_0, &10_0000000);
        oracle_client.set_price(&underlying_1, &5_0000000);

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let collateral_amount = 17_8000000;
        let liability_amount = 20_0000000;
        let positions: Positions = Positions {
            collateral: map![&e, (reserve_config_0.index, collateral_amount)],
            liabilities: map![&e, (reserve_config_1.index, liability_amount)],
            supply: map![&e],
        };
        let auction_data = AuctionData {
            bid: map![&e],
            lot: map![&e],
            block: 100,
        };
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );

            delete_liquidation(&e, &samwise);
            assert!(!storage::has_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise
            ));
        });
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "ContractError(10)")]
    fn test_delete_user_liquidation_invalid_hf() {
        let e = Env::default();
        e.mock_all_auths();
        let pool_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config_0, reserve_data_0) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(
            &e,
            &pool_id,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_id,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);
        oracle_client.set_price(&underlying_0, &10_0000000);
        oracle_client.set_price(&underlying_1, &5_0000000);

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let collateral_amount = 15_0000000;
        let liability_amount = 20_0000000;
        let positions: Positions = Positions {
            collateral: map![&e, (reserve_config_0.index, collateral_amount)],
            liabilities: map![&e, (reserve_config_1.index, liability_amount)],
            supply: map![&e],
        };
        let auction_data = AuctionData {
            bid: map![&e],
            lot: map![&e],
            block: 100,
        };
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &positions);

            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );
            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );

            delete_liquidation(&e, &samwise);
            assert!(storage::has_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise
            ));
        });
    }

    #[test]
    fn test_fill() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 1,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_expiration: 10,
                min_persistent_entry_expiration: 10,
                max_entry_expiration: 2000000,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 1_000_0000);
            let has_auction = storage::has_auction(&e, &0, &samwise);
            assert_eq!(has_auction, false);
        });
    }

    #[test]
    fn test_partial_fill() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 1,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_expiration: 10,
                min_persistent_entry_expiration: 10,
                max_entry_expiration: 2000000,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 2500000);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 9281250)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 22_9196497),
                    (underlying_1.clone(), 1_1546805)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);
        });
    }

    #[test]
    fn test_partial_partial_full_fill() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta(&e);

        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta(&e);

        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta(&e);

        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 100_000_0000)],
            lot: map![
                &e,
                (underlying_0.clone(), 10_000_0000),
                (underlying_1.clone(), 1_000_0000)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 30_000_0000),
                (reserve_config_1.index, 3_000_0000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 200_000_0000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            // Partial fill 1 - 25% @ 50% lot mod
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 100 * 5,
                protocol_version: 1,
                sequence_number: 176 + 100,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_expiration: 10,
                min_persistent_entry_expiration: 10,
                max_entry_expiration: 2000000,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 2500000);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 75_000_0000)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 7_500_0000),
                    (underlying_1.clone(), 750_0000)
                ],
                block: 176,
            };

            // Partial fill 2 - 66% @ 100% mods
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 1,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_expiration: 10,
                min_persistent_entry_expiration: 10,
                max_entry_expiration: 2000000,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 6666667);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 24_999_9975)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 2_499_9998),
                    (underlying_1.clone(), 250_0000)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);

            // full fill at 50% bid mod
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 300 * 5,
                protocol_version: 1,
                sequence_number: 176 + 300,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_expiration: 10,
                min_persistent_entry_expiration: 10,
                max_entry_expiration: 2000000,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 1_000_0000);
            let new_auction = storage::has_auction(&e, &0, &samwise);
            assert_eq!(new_auction, false);
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            assert_eq!(
                samwise_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                30_000_0000 - 1_250_0000 - 5_000_0002 - 2_499_9998
            );
            assert_eq!(
                samwise_positions
                    .collateral
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                3_000_0000 - 125_0000 - 500_0000 - 250_0000
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_2.index)
                    .unwrap_optimized(),
                200_000_0000 - 25_000_0000 - 50_000_0025 - 12_499_9988
            );
        });
    }

    #[test]
    // #[should_panic(expected = "ContractError(2)")]
    #[should_panic]
    fn test_fill_fails_pct_too_large() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 1,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_expiration: 10,
                min_persistent_entry_expiration: 10,
                max_entry_expiration: 2000000,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 2_5000000);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 9281250)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 22_9196497),
                    (underlying_1.clone(), 1_1546805)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);
        });
    }

    #[test]
    // #[should_panic(expected = "ContractError(2)")]
    #[should_panic]
    fn test_fill_fails_pct_too_small() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta(&e);

        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta(&e);

        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();
        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 1,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_expiration: 10,
                min_persistent_entry_expiration: 10,
                max_entry_expiration: 2000000,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 0);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 9281250)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 22_9196497),
                    (underlying_1.clone(), 1_1546805)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);
        });
    }

    #[test]
    fn test_apply_fill_modifiers() {
        let e = Env::default();
        let underlying_0 = Address::random(&e);
        let underlying_1 = Address::random(&e);

        let mut auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        apply_fill_modifiers(&e, &mut auction_data);
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_0.clone()),
            100_0000000
        );
        assert_eq!(auction_data.lot.len(), 0);
        auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        apply_fill_modifiers(&e, &mut auction_data);
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_0.clone()),
            100_0000000
        );
        assert_eq!(
            auction_data.lot.get_unchecked(underlying_1.clone()),
            50_0000000
        );
        auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1200,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        apply_fill_modifiers(&e, &mut auction_data);
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_0.clone()),
            100_0000000
        );
        assert_eq!(
            auction_data.lot.get_unchecked(underlying_1.clone()),
            100_0000000
        );
        auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1201,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        apply_fill_modifiers(&e, &mut auction_data);
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_0.clone()),
            99_5000000
        );
        assert_eq!(
            auction_data.lot.get_unchecked(underlying_1.clone()),
            100_0000000
        );
        auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1300,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        apply_fill_modifiers(&e, &mut auction_data);
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_0.clone()),
            50_0000000
        );
        assert_eq!(
            auction_data.lot.get_unchecked(underlying_1.clone()),
            100_0000000
        );
        auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1400,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        apply_fill_modifiers(&e, &mut auction_data);
        assert_eq!(auction_data.bid.len(), 0);
        assert_eq!(auction_data.lot.get_unchecked(underlying_1), 100_0000000);
    }

    #[test]
    fn test_apply_fill_pct() {
        let e = Env::default();
        let underlying_0 = Address::random(&e);
        let underlying_1 = Address::random(&e);

        let mut auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };
        apply_fill_pct(&e, &mut auction_data, 1_0000000);
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_0.clone()),
            100_0000000
        );
        assert_eq!(
            auction_data.lot.get_unchecked(underlying_1.clone()),
            100_0000000
        );
        auction_data = AuctionData {
            bid: map![
                &e,
                (underlying_0.clone(), 100_0000001),
                (underlying_1.clone(), 100_0000001)
            ],
            lot: map![
                &e,
                (underlying_1.clone(), 100_0000001),
                (underlying_0.clone(), 100_0000001)
            ],
            block: 1000,
        };
        let expected_new_auction = AuctionData {
            bid: map![
                &e,
                (underlying_0.clone(), 75_0000000),
                (underlying_1.clone(), 75_0000000)
            ],
            lot: map![
                &e,
                (underlying_1.clone(), 75_0000001),
                (underlying_0.clone(), 75_0000001)
            ],
            block: 1000,
        };
        let new_auction = apply_fill_pct(&e, &mut auction_data, 2500000);
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_0.clone()),
            25_0000001
        );
        assert_eq!(
            auction_data.bid.get_unchecked(underlying_1.clone()),
            25_0000001
        );
        assert_eq!(
            auction_data.lot.get_unchecked(underlying_0.clone()),
            25_0000000
        );
        assert_eq!(
            auction_data.lot.get_unchecked(underlying_1.clone()),
            25_0000000
        );
        assert_eq!(new_auction.bid, expected_new_auction.bid);
        assert_eq!(new_auction.lot, expected_new_auction.lot);
        // test rounding
        auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 1)],
            lot: map![&e, (underlying_1.clone(), 1)],
            block: 1000,
        };
        let expected_new_auction = AuctionData {
            bid: map![&e],
            lot: map![&e, (underlying_1.clone(), 1)],
            block: 1000,
        };
        let new_auction_data = apply_fill_pct(&e, &mut auction_data, 5000000);
        assert_eq!(auction_data.bid.get_unchecked(underlying_0.clone()), 1);
        assert_eq!(auction_data.lot.get(underlying_1.clone()), None);
        assert_eq!(new_auction_data.bid, expected_new_auction.bid);
        assert_eq!(new_auction_data.lot, expected_new_auction.lot);
    }
}
