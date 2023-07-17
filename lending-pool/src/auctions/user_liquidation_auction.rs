use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::unwrap::UnwrapOptimized;
use soroban_sdk::{map, panic_with_error, Address, Env};

use crate::auctions::auction::AuctionData;
use crate::constants::{SCALAR_7, SCALAR_9};
use crate::pool::Pool;
use crate::validator::require_nonnegative;
use crate::{dependencies::OracleClient, errors::PoolError, storage};

use super::{AuctionType, LiquidationMetadata};

// TODO: Revalidate math with alternative decimal reserve
pub fn create_user_liq_auction_data(
    e: &Env,
    user: &Address,
    mut liq_data: LiquidationMetadata,
) -> AuctionData {
    if storage::has_auction(e, &(AuctionType::UserLiquidation as u32), &user) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }

    let mut liquidation_quote = AuctionData {
        assets: map![e],
        block: e.ledger().sequence() + 1,
    };

    let pool = Pool::load(e);
    let oracle_client = OracleClient::new(e, &pool.config.oracle);
    let oracle_decimals = oracle_client.decimals();
    let oracle_scalar = 10i128.pow(oracle_decimals);

    let user_positions = storage::get_user_positions(e, user);
    let reserve_list = storage::get_res_list(e);
    let mut all_collateral = true;
    let mut all_liabilities = true;
    let mut collateral_base = 0;
    let mut collateral_raw = 0;
    let mut sell_collat_base = 0;
    let mut scaled_cf = 0;
    let mut liability_base = 0;
    let mut buy_liab_base = 0;
    let mut scaled_lf = 0;
    for i in 0..reserve_list.len() {
        let b_token_balance = user_positions.get_collateral(i);
        let d_token_balance = user_positions.get_liabilities(i);
        if b_token_balance == 0 && d_token_balance == 0 {
            continue;
        }
        let res_asset_address = reserve_list.get_unchecked(i).unwrap_optimized();
        let reserve = pool.load_reserve(e, &res_asset_address);
        let reserve_scalar = reserve.scalar;
        let asset_to_base = i128(oracle_client.get_price(&reserve.asset));

        if b_token_balance > 0 {
            // append users effective collateral to collateral_base
            let asset_collateral = reserve.to_effective_asset_from_b_token(b_token_balance);
            let asset_raw = reserve.to_asset_from_b_token(b_token_balance);
            collateral_base += asset_to_base
                .fixed_mul_floor(asset_collateral, reserve_scalar)
                .unwrap_optimized();
            collateral_raw += asset_to_base
                .fixed_mul_floor(asset_raw, reserve_scalar)
                .unwrap_optimized();
        }

        if d_token_balance > 0 {
            // append users effective liability to liability_base
            let asset_liability = reserve.to_effective_asset_from_d_token(d_token_balance);
            liability_base += asset_to_base
                .fixed_mul_floor(asset_liability, reserve_scalar)
                .unwrap_optimized();

            if let Some(to_buy_entry) = liq_data.liabilities.get(res_asset_address.clone()) {
                // liquidator included some amount of liabilities in the liquidation
                let to_buy_amt_d_token = to_buy_entry.unwrap_optimized();
                require_nonnegative(e, &to_buy_amt_d_token);
                liq_data
                    .liabilities
                    .remove_unchecked(res_asset_address.clone());

                // track the amount of liabilities being sold by the liquidator and the scaled liability factor for validation later
                let to_buy_amt_base = asset_to_base
                    .fixed_mul_floor(
                        reserve.to_asset_from_d_token(to_buy_amt_d_token),
                        reserve_scalar,
                    )
                    .unwrap_optimized();
                buy_liab_base += to_buy_amt_base;
                scaled_lf += to_buy_amt_base
                    .fixed_mul_floor(i128(reserve.l_factor) * 100, SCALAR_7)
                    .unwrap_optimized();
                if to_buy_amt_d_token > d_token_balance {
                    panic_with_error!(e, PoolError::InvalidBids);
                } else if to_buy_amt_d_token < d_token_balance {
                    all_liabilities = false;
                }
                liquidation_quote
                    .assets
                    .set(reserve.index, to_buy_amt_d_token);
            } else {
                all_liabilities = false;
            }
        }
    }

    // ensure the user has less collateral than liabilities
    if liability_base < collateral_base {
        panic_with_error!(e, PoolError::InvalidLiquidation);
    }

    // any remaining entries in liquidation data represent tokens that the user does not have
    if liq_data.liabilities.len() > 0 {
        panic_with_error!(e, PoolError::InvalidLiquidation);
    }

    // ensure liquidation size is fair and the collateral is large enough to allow for the auction to price the liquidation
    let weighted_cf = collateral_base
        .fixed_div_floor(collateral_raw * 100, oracle_scalar)
        .unwrap_optimized();
    // weighted_lf factor is the inverse of the liability factor
    let weighted_lf = SCALAR_9
        .fixed_div_floor(
            scaled_lf
                .fixed_div_floor(buy_liab_base, oracle_scalar)
                .unwrap_optimized(),
            SCALAR_7,
        )
        .unwrap_optimized();
    let est_incentive = (SCALAR_7
        - weighted_cf
            .fixed_div_ceil(weighted_lf, SCALAR_7)
            .unwrap_optimized())
    .fixed_div_ceil(2_0000000, SCALAR_7)
    .unwrap_optimized()
        + SCALAR_7;
    let max_target_liabilities = (liability_base
        .fixed_mul_ceil(1_0300000, SCALAR_7)
        .unwrap_optimized()
        - collateral_base)
        .fixed_div_ceil(
            weighted_lf
                .fixed_mul_floor(1_0300000, SCALAR_7)
                .unwrap_optimized()
                - weighted_cf
                    .fixed_mul_ceil(est_incentive, SCALAR_7)
                    .unwrap_optimized(),
            SCALAR_7,
        )
        .unwrap_optimized();
    let min_target_liabilities = max_target_liabilities
        .fixed_div_ceil(1_1000000, SCALAR_7)
        .unwrap_optimized(); //TODO: Assess whether 10% is an appropriate range here

    if max_target_liabilities < buy_liab_base {
        panic_with_error!(e, PoolError::InvalidBidTooLarge);
    }
    if min_target_liabilities > buy_liab_base && all_liabilities == false {
        panic_with_error!(e, PoolError::InvalidBidTooSmall);
    }
    liquidation_quote
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::auction::AuctionType,
        pool::Positions,
        storage::{self, PoolConfig},
        testutils,
    };

    use super::*;
    use soroban_sdk::testutils::{Address as AddressTestTrait, Ledger, LedgerInfo};

    #[test]
    #[should_panic(expected = "ContractError(103)")]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_address = Address::random(&e);
        let (oracle, _) = testutils::create_mock_oracle(&e);

        let samwise = Address::random(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e],
            liability: map![&e],
        };

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

            create_user_liq_auction_data(&e, &samwise, liquidation_data);
        });
    }

    #[test]
    fn test_create_user_liquidation_auction() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
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

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0, 20_0000000)],
            liability: map![&e, (underlying_2, 0_7000000)],
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
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);
            assert_eq!(result.block, 51);
            assert_eq!(
                result
                    .bid
                    .get_unchecked(reserve_config_2.index)
                    .unwrap_optimized(),
                0_7000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result
                    .lot
                    .get_unchecked(reserve_config_0.index)
                    .unwrap_optimized(),
                20_0000000
            );
            assert_eq!(result.lot.len(), 1);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(4)")]
    fn test_create_user_liquidation_auction_negative_lot_amount() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
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

        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0, 26_0000000), (underlying_1, -1_0000000)],
            liability: map![&e, (underlying_2, 0_7000000)],
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liquidation_data);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(4)")]
    fn test_create_user_liquidation_auction_negative_bid_amount() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
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
        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0, 22_0000000),],
            liability: map![&e, (underlying_2, -0_7000000)],
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liquidation_data);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(105)")]
    fn test_create_user_liquidation_auction_too_much_collateral() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
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
        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0, 33_0000000), (underlying_1, 4_5000000)],
            liability: map![&e, (underlying_2, 0_6500000)],
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liquidation_data);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(106)")]
    fn test_create_user_liquidation_auction_too_little_collateral() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
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
        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0, 15_0000000)],
            liability: map![&e, (underlying_2, 0_6500000)],
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liquidation_data);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(107)")]
    fn test_create_user_liquidation_auction_too_large() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
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
        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0, 32_0000000)],
            liability: map![&e, (underlying_2, 1_2000000)],
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liquidation_data);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(108)")]
    fn test_create_user_liquidation_auction_too_small() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
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
        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0, 17_0000000)],
            liability: map![&e, (underlying_2, 0_4500000)],
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

            e.budget().reset_unlimited();
            create_user_liq_auction_data(&e, &samwise, liquidation_data);
        });
    }

    #[test]
    fn test_fill_user_liquidation_auction() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

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

        let (underlying_2, reserve_2_asset) = testutils::create_token_contract(&e, &bombadil);
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
        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &50_0000000);

        reserve_2_asset.mint(&frodo, &0_8000000);
        reserve_2_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        let auction_data = AuctionData {
            bid: map![&e, (reserve_config_2.index, 0_7000000)],
            lot: map![&e, (reserve_config_0.index, 30_0000000)],
            block: 176,
        };
        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0.clone(), 30_0000000)],
            liability: map![&e, (underlying_2.clone(), 0_7000000)],
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
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data.clone());

            assert_eq!(result.block, 176);
            assert_eq!(
                result
                    .bid
                    .get_unchecked(reserve_config_2.index)
                    .unwrap_optimized(),
                0_7000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result
                    .lot
                    .get_unchecked(reserve_config_0.index)
                    .unwrap_optimized(),
                30_0000000
            );
            assert_eq!(result.lot.len(), 1);
            //scale up modifiers
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 175 * 5,
                protocol_version: 1,
                sequence_number: 176 + 175,
                network_id: Default::default(),
                base_reserve: 10,
            });
            let res_2_init_pool_bal = reserve_2_asset.balance(&pool_address);

            e.budget().reset_unlimited();
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.block, 351);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap_optimized(),
                (underlying_2, 0_7000177)
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(0).unwrap_optimized(),
                (underlying_0, 26_2500000)
            );
            assert_eq!(result.lot.len(), 1);
            assert_eq!(reserve_2_asset.balance(&frodo), 999823);
            assert_eq!(
                reserve_2_asset.balance(&pool_address),
                res_2_init_pool_bal + 0_7000177
            );
            let frodo_positions = storage::get_user_positions(&e, &frodo);
            assert_eq!(
                frodo_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                26_2500000
            );
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            assert_eq!(
                samwise_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                90_9100000 - 26_2500000
            );
        });
    }
    #[test]
    fn test_create_fill_user_liquidation_auction_hits_target() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        let backstop_id = Address::random(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.last_time = 12345;
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

        let (underlying_1, reserve_1_asset) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_000_000_000;
        reserve_config_1.c_factor = 0_0000000;
        reserve_config_1.l_factor = 0_7000000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        reserve_1_asset.mint(&frodo, &500_0000000_0000000);
        reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0.clone(), 3000_0000000)],
            liability: map![&e, (underlying_1.clone(), 200_7500000_0000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions = Positions {
            collateral: map![&e, (reserve_config_0.index, 3000_0000000)],
            liabilities: map![&e, (reserve_config_1.index, 200_7500000_0000000)],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data.clone());

            assert_eq!(result.block, 51);
            assert_eq!(
                result
                    .bid
                    .get_unchecked(reserve_config_1.index)
                    .unwrap_optimized(),
                200_7500000_0000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result
                    .lot
                    .get_unchecked(reserve_config_0.index)
                    .unwrap_optimized(),
                3000_0000000
            );
            assert_eq!(result.lot.len(), 1);
            //scale up modifiers
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 399 * 5,
                protocol_version: 1,
                sequence_number: 50 + 399,
                network_id: Default::default(),
                base_reserve: 10,
            });
            //liquidate user
            let auction_data = AuctionData {
                bid: map![&e, (reserve_config_1.index, 200_7500000_0000000)],
                lot: map![&e, (reserve_config_0.index, 3000_0000000)],
                block: 50,
            };
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.len(), 1);
            assert_eq!(result.block, 50 + 399);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap_optimized(),
                (underlying_1, 1_0037538_1023500)
            );
            assert_eq!(
                result.lot.get_unchecked(0).unwrap_optimized(),
                (underlying_0, 3000_0000000)
            );
            let frodo_positions = storage::get_user_positions(&e, &frodo);
            assert_eq!(
                frodo_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                3000_0000000
            );
            assert_eq!(
                reserve_1_asset.balance(&frodo),
                500_0000000_0000000 - 1_0037500_0000000 - 381023500
            );
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            assert_eq!(
                samwise_positions.collateral.get(reserve_config_0.index),
                None
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_1.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                200_7500000_0000000 - 1_0037500_0000000
            );
        });
    }
    #[test]
    fn test_liquidate_user_dust_collateral() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        let backstop_id = Address::random(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.last_time = 12345;
        reserve_data_0.b_rate = 2_100_000_000;
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

        let (underlying_1, reserve_1_asset) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_config_1.c_factor = 0_0000000;
        reserve_config_1.l_factor = 0_7000000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        reserve_1_asset.mint(&frodo, &500_0000000);
        reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &50_0000000);
        e.budget().reset_unlimited();

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0.clone(), 00_0000001)],
            liability: map![&e, (underlying_1.clone(), 2_7500000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions = Positions {
            collateral: map![&e, (reserve_config_0.index, 00_0000001)],
            liabilities: map![&e, (reserve_config_1.index, 2_7500000)],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data.clone());

            assert_eq!(result.block, 51);
            assert_eq!(
                result
                    .bid
                    .get_unchecked(reserve_config_1.index)
                    .unwrap_optimized(),
                2_7500000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result
                    .lot
                    .get_unchecked(reserve_config_0.index)
                    .unwrap_optimized(),
                00_0000001
            );
            assert_eq!(result.lot.len(), 1);
            //scale up modifiers
            e.ledger().set(LedgerInfo {
                timestamp: 12345,
                protocol_version: 1,
                sequence_number: 50 + 400,
                network_id: Default::default(),
                base_reserve: 10,
            });
            //liquidate user
            let auction_data = AuctionData {
                bid: map![&e, (reserve_config_1.index, 2_7500000)],
                lot: map![&e, (reserve_config_0.index, 00_0000001)],
                block: 50,
            };
            //TODO: fix this
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.len(), 1);
            assert_eq!(result.block, 50 + 400);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap_optimized(),
                (underlying_1, 0)
            );
            assert_eq!(
                result.lot.get_unchecked(0).unwrap_optimized(),
                (underlying_0, 00_0000001)
            );
            let frodo_positions = storage::get_user_positions(&e, &frodo);
            assert_eq!(
                frodo_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                00_0000001
            );
            assert_eq!(reserve_1_asset.balance(&frodo), 500_0000000);
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            assert_eq!(
                samwise_positions.collateral.get(reserve_config_0.index),
                None
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_1.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                2_7500000
            );
        });
    }

    #[test]
    fn test_liquidate_user_more_collateral() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        let backstop_id = Address::random(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.last_time = 12345;
        reserve_data_0.b_rate = 1_000_000_000;
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

        let (underlying_1, reserve_1_asset) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_000_000_000;
        reserve_config_1.c_factor = 0_0000000;
        reserve_config_1.l_factor = 0_7000000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        reserve_1_asset.mint(&frodo, &500_0000000_0000000);
        reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        e.budget().reset_unlimited();

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (underlying_0.clone(), 3000_0000000)],
            liability: map![&e, (underlying_1.clone(), 200_7500000_0000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions = Positions {
            collateral: map![&e, (reserve_config_0.index, 3000_0000000)],
            liabilities: map![&e, (reserve_config_1.index, 200_7500000_0000000)],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data.clone());

            assert_eq!(result.block, 51);
            assert_eq!(
                result
                    .bid
                    .get_unchecked(reserve_config_1.index)
                    .unwrap_optimized(),
                200_7500000_0000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result
                    .lot
                    .get_unchecked(reserve_config_0.index)
                    .unwrap_optimized(),
                3000_0000000
            );
            assert_eq!(result.lot.len(), 1);
            //scale up modifiers
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 399 * 5,
                protocol_version: 1,
                sequence_number: 50 + 399,
                network_id: Default::default(),
                base_reserve: 10,
            });
            //liquidate user
            let auction_data = AuctionData {
                bid: map![&e, (reserve_config_1.index, 200_7500000_0000000)],
                lot: map![&e, (reserve_config_0.index, 3000_0000000)],
                block: 50,
            };
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.len(), 1);
            assert_eq!(result.block, 50 + 399);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap_optimized(),
                (underlying_1, 100375381023500)
            );
            assert_eq!(
                result.lot.get_unchecked(0).unwrap_optimized(),
                (underlying_0, 3000_0000000)
            );
            let frodo_positions = storage::get_user_positions(&e, &frodo);
            assert_eq!(
                frodo_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                3000_0000000
            );
            assert_eq!(
                reserve_1_asset.balance(&frodo),
                500_0000000_0000000 - 1_0037500_0000000 - 381023500
            );
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            assert_eq!(
                samwise_positions.collateral.get(reserve_config_0.index),
                None
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_1.index)
                    .unwrap_optimized()
                    .unwrap_optimized(),
                200_7500000_0000000 - 1_0037500_0000000
            );
        });
    }
}
