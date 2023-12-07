use cast::i128;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::unwrap::UnwrapOptimized;
use soroban_sdk::{map, panic_with_error, Address, Env};

use crate::auctions::auction::AuctionData;
use crate::constants::SCALAR_7;
use crate::pool::{Pool, PositionData, User};
use crate::{errors::PoolError, storage};

use super::AuctionType;

// TODO: Revalidate math with alternative decimal reserve
pub fn create_user_liq_auction_data(
    e: &Env,
    user: &Address,
    percent_liquidated: u64,
) -> AuctionData {
    if storage::has_auction(e, &(AuctionType::UserLiquidation as u32), user) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }
    if percent_liquidated > 100 || percent_liquidated == 0 {
        panic_with_error!(e, PoolError::InvalidLiquidation);
    }
    let percent_liquidated_i128 = i128(percent_liquidated) * 1_00000; // scale to decimal form with 7 decimals

    let mut liquidation_quote = AuctionData {
        bid: map![e],
        lot: map![e],
        block: e.ledger().sequence() + 1,
    };
    let mut pool = Pool::load(e);
    let oracle_scalar = 10i128.pow(pool.load_price_decimals(e));

    let mut user_state = User::load(e, user);
    let reserve_list = storage::get_res_list(e);
    let position_data = PositionData::calculate_from_positions(e, &mut pool, &user_state.positions);

    // ensure the user has less collateral than liabilities
    if position_data.liability_base < position_data.collateral_base {
        panic_with_error!(e, PoolError::InvalidLiquidation);
    }

    // ensure liquidation size is fair and the collateral is large enough to allow for the auction to price the liquidation
    let avg_cf = position_data
        .collateral_base
        .fixed_div_floor(position_data.collateral_raw, oracle_scalar)
        .unwrap_optimized();
    // avg_lf is the inverse of the average liability factor
    let avg_lf = position_data
        .liability_base
        .fixed_div_floor(position_data.liability_raw, oracle_scalar)
        .unwrap_optimized();
    let est_incentive = (SCALAR_7 - avg_cf.fixed_div_ceil(avg_lf, SCALAR_7).unwrap_optimized())
        .fixed_div_ceil(2_0000000, SCALAR_7)
        .unwrap_optimized()
        + SCALAR_7;

    let est_withdrawn_collateral = position_data
        .liability_raw
        .fixed_mul_floor(percent_liquidated_i128, oracle_scalar)
        .unwrap_optimized()
        .fixed_mul_floor(est_incentive, SCALAR_7)
        .unwrap_optimized();
    let mut est_withdrawn_collateral_pct = est_withdrawn_collateral
        .fixed_div_ceil(position_data.collateral_raw, oracle_scalar)
        .unwrap_optimized();
    if est_withdrawn_collateral_pct > 1_0000000 {
        est_withdrawn_collateral_pct = 1_0000000;
    }

    for (asset, amount) in user_state.positions.collateral.iter() {
        let res_asset_address = reserve_list.get_unchecked(asset);
        // Note: we multiply balance by estimated withdrawn collateral percent to allow
        //       smoother scaling of liquidation modifiers
        let b_tokens_removed = amount
            .fixed_mul_ceil(est_withdrawn_collateral_pct, SCALAR_7)
            .unwrap_optimized();
        liquidation_quote
            .lot
            .set(res_asset_address, b_tokens_removed);
    }

    for (asset, amount) in user_state.positions.liabilities.iter() {
        let res_asset_address = reserve_list.get_unchecked(asset);
        let d_tokens_removed = amount
            .fixed_mul_ceil(percent_liquidated_i128, SCALAR_7)
            .unwrap_optimized();
        liquidation_quote
            .bid
            .set(res_asset_address, d_tokens_removed);
    }

    if percent_liquidated == 100 {
        // ensure that there isn't enough collateral to fill without fully liquidating
        if est_withdrawn_collateral < position_data.collateral_raw {
            panic_with_error!(e, PoolError::InvalidLiqTooLarge);
        }
    } else {
        user_state.rm_positions(
            e,
            &mut pool,
            liquidation_quote.lot.clone(),
            liquidation_quote.bid.clone(),
        );
        let new_hf = PositionData::calculate_from_positions(e, &mut pool, &user_state.positions)
            .as_health_factor();

        //check if liq is too large
        if new_hf > 1_1500000 {
            panic_with_error!(e, PoolError::InvalidLiqTooLarge);
        }
        // check if liq is too small
        if new_hf < 1_0300000 {
            panic_with_error!(e, PoolError::InvalidLiqTooSmall);
        }
    }
    liquidation_quote
}

pub fn fill_user_liq_auction(
    e: &Env,
    pool: &mut Pool,
    auction_data: &AuctionData,
    user: &Address,
    filler_state: &mut User,
) {
    let mut user_state = User::load(e, user);
    user_state.rm_positions(e, pool, auction_data.lot.clone(), auction_data.bid.clone());
    filler_state.add_positions(e, pool, auction_data.lot.clone(), auction_data.bid.clone());
    user_state.store(e);
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::auction::AuctionType,
        pool::Positions,
        storage::{self, PoolConfig},
        testutils::{self, create_pool},
    };

    use super::*;
    use sep_40_oracle::testutils::Asset;
    use soroban_sdk::{
        testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
        vec, Symbol,
    };

    #[test]
    #[should_panic(expected = "Error(Contract, #103)")]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_address = create_pool(&e);
        let (oracle, _) = testutils::create_mock_oracle(&e);

        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let liq_pct = 50;

        let auction_data = AuctionData {
            bid: map![&e],
            lot: map![&e],
            block: 50,
        };
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );
            create_user_liq_auction_data(&e, &samwise, liq_pct);
        });
    }

    #[test]
    fn test_create_user_liquidation_auction() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
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
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
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

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
                Asset::Stellar(underlying_2.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 50_0000000]);

        let liq_pct = 45;
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
            let result = create_user_liq_auction_data(&e, &samwise, liq_pct);
            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(underlying_2), 1_2375000);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.get_unchecked(underlying_0), 30_5595329);
            assert_eq!(result.lot.get_unchecked(underlying_1), 1_5395739);
            assert_eq!(result.lot.len(), 2);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #105)")]
    fn test_create_user_liquidation_auction_bad_full_liq() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
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
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
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
        e.budget().reset_unlimited();

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0),
                Asset::Stellar(underlying_1),
                Asset::Stellar(underlying_2),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 50_0000000]);

        let liq_pct = 100;
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liq_pct);
        });
    }
    #[test]
    #[should_panic(expected = "Error(Contract, #105)")]
    fn test_create_user_liquidation_auction_too_large() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
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
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
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
        e.budget().reset_unlimited();

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0),
                Asset::Stellar(underlying_1),
                Asset::Stellar(underlying_2),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 50_0000000]);

        let liq_pct = 46;
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liq_pct);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #106)")]
    fn test_create_user_liquidation_auction_too_small() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
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
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
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
        e.budget().reset_unlimited();

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0),
                Asset::Stellar(underlying_1),
                Asset::Stellar(underlying_2),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 50_0000000]);

        let liq_pct = 25;
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liq_pct);
        });
    }

    #[test]
    fn test_fill_user_liquidation_auction() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 17280,
            min_persistent_entry_ttl: 17280,
            max_entry_ttl: 9999999,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
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

        let (underlying_2, reserve_2_asset) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
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
        e.budget().reset_unlimited();

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
                Asset::Stellar(underlying_2.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 50_0000000]);

        reserve_2_asset.mint(&frodo, &0_8000000);
        reserve_2_asset.approve(&frodo, &pool_address, &i128::MAX, &1000000);

        let mut auction_data = AuctionData {
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

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 20,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 17280,
                min_persistent_entry_ttl: 17280,
                max_entry_ttl: 9999999,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill_user_liq_auction(&e, &mut pool, &mut auction_data, &samwise, &mut frodo_state);
            let frodo_positions = frodo_state.positions;
            assert_eq!(
                frodo_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                30_5595329
            );
            assert_eq!(
                frodo_positions
                    .collateral
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                1_5395739
            );
            assert_eq!(
                frodo_positions
                    .liabilities
                    .get(reserve_config_2.index)
                    .unwrap_optimized(),
                1_2375000
            );
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            assert_eq!(
                samwise_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                90_9100000 - 30_5595329
            );
            assert_eq!(
                samwise_positions
                    .collateral
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                04_5800000 - 1_5395739
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_2.index)
                    .unwrap_optimized(),
                02_7500000 - 1_2375000
            );
        });
    }

    #[test]
    fn test_fill_user_liquidation_auction_hits_target() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 17280,
            min_persistent_entry_ttl: 17280,
            max_entry_ttl: 9999999,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
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

        let (underlying_2, reserve_2_asset) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
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
        e.budget().reset_unlimited();

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
                Asset::Stellar(underlying_2.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 50_0000000]);

        reserve_2_asset.mint(&frodo, &0_8000000);
        reserve_2_asset.approve(&frodo, &pool_address, &i128::MAX, &1000000);

        let mut auction_data = AuctionData {
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
            //scale up modifiers
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 20,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 17280,
                min_persistent_entry_ttl: 17280,
                max_entry_ttl: 9999999,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill_user_liq_auction(&e, &mut pool, &mut auction_data, &samwise, &mut frodo_state);
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            let samwise_hf =
                PositionData::calculate_from_positions(&e, &mut pool, &samwise_positions)
                    .as_health_factor();
            assert_eq!(samwise_hf, 1_1458978);
        });
    }
}
