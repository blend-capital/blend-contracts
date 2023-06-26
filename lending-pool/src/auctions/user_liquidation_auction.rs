use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::unwrap::UnwrapOptimized;
use soroban_sdk::{map, panic_with_error, vec, Address, Env};

use crate::auctions::auction::AuctionData;
use crate::constants::{SCALAR_7, SCALAR_9};
use crate::emissions;
use crate::pool::Pool;
use crate::validator::require_nonnegative;
use crate::{dependencies::OracleClient, errors::PoolError, storage};

use super::{fill_debt_token, get_fill_modifiers, AuctionQuote, AuctionType, LiquidationMetadata};

// TODO: Revalidate math with alternative decimal reserve
pub fn create_user_liq_auction_data(
    e: &Env,
    user: &Address,
    mut liq_data: LiquidationMetadata,
) -> AuctionData {
    if storage::has_auction(e, &(AuctionType::UserLiquidation as u32), &user) {
        panic_with_error!(e, PoolError::AlreadyInitialized);
    }

    let mut liquidation_quote = AuctionData {
        bid: map![e],
        lot: map![e],
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
            collateral_base += asset_to_base
                .fixed_mul_floor(asset_collateral, reserve_scalar)
                .unwrap_optimized();
            if let Some(to_sell_entry) = liq_data.collateral.get(res_asset_address.clone()) {
                // liquidator included some amount of collateral in the liquidation
                let to_sell_amt_b_token = to_sell_entry.unwrap_optimized();
                require_nonnegative(e, &to_sell_amt_b_token);
                liq_data
                    .collateral
                    .remove_unchecked(res_asset_address.clone());

                // track the amount of collateral being purchased by the liquidator and the scaled collateral factor for validation later
                let to_sell_amt_base = asset_to_base
                    .fixed_mul_floor(
                        reserve.to_asset_from_b_token(to_sell_amt_b_token),
                        reserve_scalar,
                    )
                    .unwrap_optimized();
                sell_collat_base += to_sell_amt_base;

                scaled_cf += to_sell_amt_base
                    .fixed_mul_floor(i128(reserve.c_factor) * 100, SCALAR_7)
                    .unwrap_optimized();
                if to_sell_amt_b_token > b_token_balance {
                    panic_with_error!(e, PoolError::InvalidLot);
                } else if to_sell_amt_b_token < b_token_balance {
                    all_collateral = false;
                }
                liquidation_quote
                    .lot
                    .set(reserve.index, to_sell_amt_b_token);
            } else {
                all_collateral = false;
            }
        }

        if d_token_balance > 0 {
            // append users effective liability to liability_base
            let asset_liability = reserve.to_effective_asset_from_d_token(d_token_balance);
            liability_base += asset_to_base
                .fixed_mul_floor(asset_liability, reserve_scalar)
                .unwrap_optimized();

            if let Some(to_buy_entry) = liq_data.liability.get(res_asset_address.clone()) {
                // liquidator included some amount of liabilities in the liquidation
                let to_buy_amt_d_token = to_buy_entry.unwrap_optimized();
                require_nonnegative(e, &to_buy_amt_d_token);
                liq_data
                    .liability
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
                liquidation_quote.bid.set(reserve.index, to_buy_amt_d_token);
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
    if liq_data.collateral.len() > 0 || liq_data.liability.len() > 0 {
        panic_with_error!(e, PoolError::InvalidLiquidation);
    }

    // ensure liquidation size is fair and the collateral is large enough to allow for the auction to price the liquidation
    let weighted_cf = scaled_cf
        .fixed_div_floor(sell_collat_base * 100, oracle_scalar)
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

    let max_collateral_lot = buy_liab_base
        .fixed_mul_floor(2_5000000, SCALAR_7)
        .unwrap_optimized();
    let min_collateral_lot = buy_liab_base
        .fixed_mul_floor(1_2500000, SCALAR_7)
        .unwrap_optimized();

    if max_target_liabilities < buy_liab_base {
        panic_with_error!(e, PoolError::InvalidBidTooLarge);
    }
    if min_target_liabilities > buy_liab_base && all_liabilities == false {
        panic_with_error!(e, PoolError::InvalidBidTooSmall);
    }
    if sell_collat_base > max_collateral_lot {
        panic_with_error!(e, PoolError::InvalidLotTooLarge);
    }
    if sell_collat_base < min_collateral_lot && all_collateral == false {
        panic_with_error!(e, PoolError::InvalidLotTooSmall);
    }

    liquidation_quote
}

pub fn fill_user_liq_auction(
    e: &Env,
    auction_data: &AuctionData,
    user: &Address,
    filler: &Address,
) -> AuctionQuote {
    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };
    let (bid_modifier, lot_modifier) = get_fill_modifiers(e, auction_data);

    let mut pool = Pool::load(e);
    let mut user_positions = storage::get_user_positions(e, user);
    let mut filler_positions = storage::get_user_positions(e, filler);

    let reserve_list = storage::get_res_list(e);
    for i in 0..reserve_list.len() {
        let bid_amount = auction_data.bid.get(i).unwrap_or(Ok(0)).unwrap_optimized();
        let lot_amount = auction_data.lot.get(i).unwrap_or(Ok(0)).unwrap_optimized();
        if bid_amount == 0 && lot_amount == 0 {
            continue;
        }

        let res_asset_address = reserve_list.get_unchecked(i).unwrap_optimized();
        let reserve = pool.load_reserve(e, &res_asset_address);

        // bids are liabilities stored as debtTokens
        if bid_amount > 0 {
            let mod_bid_amount = bid_amount
                .fixed_mul_floor(bid_modifier, SCALAR_7)
                .unwrap_optimized();
            let underlying_amount = fill_debt_token(
                e,
                &mut pool,
                &user,
                &filler,
                &res_asset_address,
                mod_bid_amount,
                &mut user_positions,
            );
            auction_quote
                .bid
                .push_back((res_asset_address, underlying_amount));
        }

        // lot contains collateral stored as blendTokens
        if lot_amount > 0 {
            // pay out lot in blendTokens by transferring them from
            // the liquidated user to the auction filler
            let mod_lot_amount = lot_amount
                .fixed_mul_floor(lot_modifier, SCALAR_7)
                .unwrap_optimized();
            // update both the filler and liquidated user's emissions
            // @dev: TODO: The reserve emissions update will short circuit on the second go,
            //       but this can be optimized to avoid a second read
            emissions::update_emissions(
                e,
                reserve.index * 2 + 1,
                reserve.b_supply,
                reserve.scalar,
                user,
                user_positions.get_total_supply(reserve.index),
                false,
            );
            emissions::update_emissions(
                e,
                reserve.index * 2 + 1,
                reserve.b_supply,
                reserve.scalar,
                filler,
                filler_positions.get_total_supply(reserve.index),
                false,
            );
            user_positions.remove_collateral(e, reserve.index, mod_lot_amount);
            // TODO: Consider returning supply to avoid any required health check on withdrawal
            filler_positions.add_collateral(reserve.index, mod_lot_amount);
            // TODO: Is this confusing to quote in blendTokens?
            auction_quote
                .lot
                .push_back((reserve.asset.clone(), mod_lot_amount));
        }

        reserve.store(e);
    }
    storage::set_user_positions(e, user, &user_positions);
    storage::set_user_positions(e, filler, &filler_positions);

    auction_quote
}

// #[cfg(test)]
// mod tests {

//     use crate::{
//         auctions::auction::AuctionType,
//         storage::{self, PoolConfig},
//         testutils::{create_mock_oracle, create_reserve, setup_reserve},
//     };

//     use super::*;
//     use soroban_sdk::testutils::{Address as AddressTestTrait, Ledger, LedgerInfo};

//     #[test]
//     fn test_create_interest_auction_already_in_progress() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let (oracle, _) = create_mock_oracle(&e);

//         let samwise = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 100,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e],
//             liability: map![&e],
//         };

//         let auction_data = AuctionData {
//             bid: map![&e],
//             lot: map![&e],
//             block: 50,
//         };
//         let pool_config = PoolConfig {
//             oracle,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             storage::set_pool_config(&e, &pool_config);
//             storage::set_auction(
//                 &e,
//                 &(AuctionType::UserLiquidation as u32),
//                 &samwise,
//                 &auction_data,
//             );

//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

//             match result {
//                 Ok(_) => assert!(false),
//                 Err(err) => assert_eq!(err, PoolError::AuctionInProgress),
//             };
//         });
//     }

//     #[test]
//     fn test_create_user_liquidation_auction() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);

//         let pool_address = Address::random(&e);
//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset, 20_0000000)],
//             liability: map![&e, (reserve_2.asset, 0_7000000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data).unwrap_optimized();
//             assert_eq!(result.block, 51);
//             assert_eq!(
//                 result.bid.get_unchecked(reserve_2.config.index).unwrap_optimized(),
//                 0_7000000
//             );
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(
//                 result.lot.get_unchecked(reserve_0.config.index).unwrap_optimized(),
//                 20_0000000
//             );
//             assert_eq!(result.lot.len(), 1);
//         });
//     }

//     #[test]
//     fn test_create_user_liquidation_auction_negative_lot_amount() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![
//                 &e,
//                 (reserve_0.asset, 26_0000000),
//                 (reserve_1.asset, -1_0000000)
//             ],
//             liability: map![&e, (reserve_2.asset, 0_7000000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);
//             match result {
//                 Ok(_) => assert!(false),
//                 Err(err) => match err {
//                     PoolError::NegativeAmount => assert!(true),
//                     _ => assert!(false),
//                 },
//             }
//         });
//     }

//     #[test]
//     fn test_create_user_liquidation_auction_negative_bid_amount() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset, 22_0000000),],
//             liability: map![&e, (reserve_2.asset, -0_7000000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);
//             match result {
//                 Ok(_) => assert!(false),
//                 Err(err) => match err {
//                     PoolError::NegativeAmount => assert!(true),
//                     _ => assert!(false),
//                 },
//             }
//         });
//     }

//     #[test]
//     fn test_create_user_liquidation_auction_too_much_collateral() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![
//                 &e,
//                 (reserve_0.asset, 33_0000000),
//                 (reserve_1.asset, 4_5000000)
//             ],
//             liability: map![&e, (reserve_2.asset, 0_6500000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

//             match result {
//                 Ok(_) => assert!(false),
//                 Err(err) => assert_eq!(err, PoolError::InvalidLotTooLarge),
//             };
//         });
//     }

//     #[test]
//     fn test_create_user_liquidation_auction_too_little_collateral() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset, 15_0000000)],
//             liability: map![&e, (reserve_2.asset, 0_6500000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

//             match result {
//                 Ok(_) => assert!(false),
//                 Err(err) => assert_eq!(err, PoolError::InvalidLotTooSmall),
//             };
//         });
//     }

//     #[test]
//     fn test_create_user_liquidation_auction_too_large() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset, 32_0000000)],
//             liability: map![&e, (reserve_2.asset, 1_2000000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

//             match result {
//                 Ok(_) => assert!(false),
//                 Err(err) => assert_eq!(err, PoolError::InvalidBidTooLarge),
//             };
//         });
//     }

//     #[test]
//     fn test_create_user_liquidation_auction_too_small() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset, 17_0000000)],
//             liability: map![&e, (reserve_2.asset, 0_4500000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

//             match result {
//                 Ok(_) => assert!(false),
//                 Err(err) => assert_eq!(err, PoolError::InvalidBidTooSmall),
//             };
//         });
//     }

//     #[test]
//     fn test_fill_user_liquidation_auction() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 175,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);
//         let frodo = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();
//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_200_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_7500000;
//         reserve_1.config.l_factor = 0_7500000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

//         let mut reserve_2 = create_reserve(&e);
//         reserve_2.data.last_time = 12345;
//         reserve_2.config.c_factor = 0_0000000;
//         reserve_2.config.l_factor = 0_7000000;
//         reserve_2.config.index = 2;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
//         let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &4_0000000);
//         oracle_client.set_price(&reserve_2.asset, &50_0000000);

//         let reserve_2_asset = TokenClient::new(&e, &reserve_2.asset);
//         reserve_2_asset.mint(&frodo, &0_5000000);
//         reserve_2_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

//         let auction_data = AuctionData {
//             bid: map![&e, (reserve_2.config.index, 0_5000000)],
//             lot: map![&e, (reserve_0.config.index, 18_1818181)],
//             block: 50,
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_supply(1, true);
//             user_config.set_liability(2, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);

//             b_token_0.mint(&samwise, &90_9100000);
//             b_token_1.mint(&samwise, &04_5800000);
//             d_token_2.mint(&samwise, &02_7500000);
//             let res_2_init_pool_bal = reserve_2_asset.balance(&pool_address);

//             e.budget().reset_unlimited();
//             let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);

//             assert_eq!(result.block, 175);
//             assert_eq!(
//                 result.bid.get_unchecked(0).unwrap_optimized(),
//                 (reserve_2.asset, 0_5000000)
//             );
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(
//                 result.lot.get_unchecked(0).unwrap_optimized(),
//                 (reserve_0.config.b_token, 11_3636363)
//             );
//             assert_eq!(result.lot.len(), 1);
//             assert_eq!(reserve_2_asset.balance(&frodo), 0);
//             assert_eq!(
//                 reserve_2_asset.balance(&pool_address),
//                 res_2_init_pool_bal + 0_5000000
//             );
//             assert_eq!(b_token_0.balance(&frodo), 11_3636363);
//             assert_eq!(b_token_0.balance(&samwise), 79_5463637);
//         });
//     }
//     #[test]
//     fn test_create_fill_user_liquidation_auction_hits_target() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);
//         let frodo = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         let backstop_id = Address::random(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();

//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_000_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_000_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_0000000;
//         reserve_1.config.l_factor = 0_7000000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
//         let reserve_1_asset = TokenClient::new(&e, &reserve_1.asset);
//         reserve_1_asset.mint(&frodo, &500_0000000_0000000);
//         reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset.clone(), 3000_0000000)],
//             liability: map![&e, (reserve_1.asset.clone(), 200_7500000_0000000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_liability(1, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);
//             storage::set_backstop(&e, &backstop_id);

//             b_token_0.mint(&samwise, &3000_0000000);
//             d_token_1.mint(&samwise, &200_7500000_0000000);

//             e.budget().reset_unlimited();
//             let result =
//                 create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap_optimized();

//             assert_eq!(result.block, 51);
//             assert_eq!(
//                 result.bid.get_unchecked(reserve_1.config.index).unwrap_optimized(),
//                 200_7500000_0000000
//             );
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(
//                 result.lot.get_unchecked(reserve_0.config.index).unwrap_optimized(),
//                 3000_0000000
//             );
//             assert_eq!(result.lot.len(), 1);
//             //scale up modifiers
//             e.ledger().set(LedgerInfo {
//                 timestamp: 12345 + 399 * 5,
//                 protocol_version: 1,
//                 sequence_number: 50 + 399,
//                 network_id: Default::default(),
//                 base_reserve: 10,
//             });
//             //liquidate user
//             let auction_data = AuctionData {
//                 bid: map![&e, (reserve_1.config.index, 200_7500000_0000000)],
//                 lot: map![&e, (reserve_0.config.index, 3000_0000000)],
//                 block: 50,
//             };
//             let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(result.lot.len(), 1);
//             assert_eq!(result.block, 50 + 399);
//             assert_eq!(
//                 result.bid.get_unchecked(0).unwrap_optimized(),
//                 (reserve_1.asset, 1_0037500_0000000)
//             );
//             assert_eq!(
//                 result.lot.get_unchecked(0).unwrap_optimized(),
//                 (reserve_0.config.b_token, 3000_0000000)
//             );
//             assert_eq!(b_token_0.balance(&frodo), 3000_0000000);
//             assert_eq!(
//                 reserve_1_asset.balance(&frodo),
//                 500_0000000_0000000 - 1_0037500_0000000 - 381023500
//             );
//             assert_eq!(b_token_0.balance(&samwise), 00_0000000);
//             assert_eq!(
//                 d_token_1.balance(&samwise),
//                 200_7500000_0000000 - 1_0037500_0000000
//             );
//         });
//     }
//     #[test]
//     fn test_liquidate_user_dust_collateral() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);
//         let frodo = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         let backstop_id = Address::random(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();

//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(2_100_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_000_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_0000000;
//         reserve_1.config.l_factor = 0_7000000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
//         let reserve_1_asset = TokenClient::new(&e, &reserve_1.asset);
//         reserve_1_asset.mint(&frodo, &500_0000000);
//         reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset.clone(), 00_0000001)],
//             liability: map![&e, (reserve_1.asset.clone(), 2_7500000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_liability(1, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);
//             storage::set_backstop(&e, &backstop_id);

//             b_token_0.mint(&samwise, &00_0000001);
//             d_token_1.mint(&samwise, &02_7500000);

//             e.budget().reset_unlimited();
//             let result =
//                 create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap_optimized();

//             assert_eq!(result.block, 51);
//             assert_eq!(
//                 result.bid.get_unchecked(reserve_1.config.index).unwrap_optimized(),
//                 2_7500000
//             );
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(
//                 result.lot.get_unchecked(reserve_0.config.index).unwrap_optimized(),
//                 00_0000001
//             );
//             assert_eq!(result.lot.len(), 1);
//             //scale up modifiers
//             e.ledger().set(LedgerInfo {
//                 timestamp: 12345,
//                 protocol_version: 1,
//                 sequence_number: 50 + 400,
//                 network_id: Default::default(),
//                 base_reserve: 10,
//             });
//             //liquidate user
//             let auction_data = AuctionData {
//                 bid: map![&e, (reserve_1.config.index, 2_7500000)],
//                 lot: map![&e, (reserve_0.config.index, 00_0000001)],
//                 block: 50,
//             };
//             //TODO: fix this
//             let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(result.lot.len(), 1);
//             assert_eq!(result.block, 50 + 400);
//             assert_eq!(result.bid.get_unchecked(0).unwrap_optimized(), (reserve_1.asset, 0));
//             assert_eq!(
//                 result.lot.get_unchecked(0).unwrap_optimized(),
//                 (reserve_0.config.b_token, 00_0000001)
//             );
//             assert_eq!(b_token_0.balance(&frodo), 00_0000001);
//             assert_eq!(reserve_1_asset.balance(&frodo), 500_0000000);
//             assert_eq!(b_token_0.balance(&samwise), 00_0000000);
//             assert_eq!(d_token_1.balance(&samwise), 2_7500000);
//         });
//     }
//     #[test]
//     fn test_liquidate_user_more_collateral() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);
//         let frodo = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         let backstop_id = Address::random(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();

//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_000_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_000_000_000);
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_0000000;
//         reserve_1.config.l_factor = 0_7000000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
//         let reserve_1_asset = TokenClient::new(&e, &reserve_1.asset);
//         reserve_1_asset.mint(&frodo, &500_0000000_0000000);
//         reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset.clone(), 3000_0000000)],
//             liability: map![&e, (reserve_1.asset.clone(), 200_7500000_0000000)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_liability(1, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);
//             storage::set_backstop(&e, &backstop_id);

//             b_token_0.mint(&samwise, &3000_0000000);
//             d_token_1.mint(&samwise, &200_7500000_0000000);

//             e.budget().reset_unlimited();
//             let result =
//                 create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap_optimized();

//             assert_eq!(result.block, 51);
//             assert_eq!(
//                 result.bid.get_unchecked(reserve_1.config.index).unwrap_optimized(),
//                 200_7500000_0000000
//             );
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(
//                 result.lot.get_unchecked(reserve_0.config.index).unwrap_optimized(),
//                 3000_0000000
//             );
//             assert_eq!(result.lot.len(), 1);
//             //scale up modifiers
//             e.ledger().set(LedgerInfo {
//                 timestamp: 12345 + 399 * 5,
//                 protocol_version: 1,
//                 sequence_number: 50 + 399,
//                 network_id: Default::default(),
//                 base_reserve: 10,
//             });
//             //liquidate user
//             let auction_data = AuctionData {
//                 bid: map![&e, (reserve_1.config.index, 200_7500000_0000000)],
//                 lot: map![&e, (reserve_0.config.index, 3000_0000000)],
//                 block: 50,
//             };
//             let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(result.lot.len(), 1);
//             assert_eq!(result.block, 50 + 399);
//             assert_eq!(
//                 result.bid.get_unchecked(0).unwrap_optimized(),
//                 (reserve_1.asset, 1_0037500_0000000)
//             );
//             assert_eq!(
//                 result.lot.get_unchecked(0).unwrap_optimized(),
//                 (reserve_0.config.b_token, 3000_0000000)
//             );
//             assert_eq!(b_token_0.balance(&frodo), 3000_0000000);
//             assert_eq!(
//                 reserve_1_asset.balance(&frodo),
//                 500_0000000_0000000 - 1_0037500_0000000 - 381023500
//             );
//             assert_eq!(b_token_0.balance(&samwise), 00_0000000);
//             assert_eq!(
//                 d_token_1.balance(&samwise),
//                 200_7500000_0000000 - 1_0037500_0000000
//             );
//         });
//     }
//     #[test]
//     fn test_liquidate_user_check_pulldown() {
//         let e = Env::default();

//         e.mock_all_auths();
//         e.ledger().set(LedgerInfo {
//             timestamp: 12345,
//             protocol_version: 1,
//             sequence_number: 50,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let bombadil = Address::random(&e);
//         let samwise = Address::random(&e);
//         let frodo = Address::random(&e);

//         let pool_address = Address::random(&e);

//         let (oracle_address, oracle_client) = create_mock_oracle(&e);

//         let backstop_id = Address::random(&e);

//         // creating reserves for a pool exhausts the budget
//         e.budget().reset_unlimited();

//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.b_rate = Some(1_300_000_000);
//         reserve_0.data.last_time = 12345;
//         reserve_0.config.c_factor = 0_8500000;
//         reserve_0.config.l_factor = 0_9000000;
//         reserve_0.config.index = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
//         let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.b_rate = Some(1_600_000_000);
//         reserve_1.data.d_rate = 2_100_000_000;
//         reserve_1.data.last_time = 12345;
//         reserve_1.config.c_factor = 0_1000000;
//         reserve_1.config.l_factor = 0_7000000;
//         reserve_1.config.index = 1;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
//         let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);

//         e.budget().reset_unlimited();

//         oracle_client.set_price(&reserve_0.asset, &2_0000000);
//         oracle_client.set_price(&reserve_1.asset, &50_0000000);

//         let liquidation_data = LiquidationMetadata {
//             collateral: map![&e, (reserve_0.asset.clone(), 2)],
//             liability: map![&e, (reserve_1.asset.clone(), 1)],
//         };
//         let pool_config = PoolConfig {
//             oracle: oracle_address,
//             bstop_rate: 0_100_000_000,
//             status: 0,
//         };
//         e.as_contract(&pool_address, || {
//             let mut user_config = ReserveUsage::new(0);
//             user_config.set_supply(0, true);
//             user_config.set_liability(1, true);
//             storage::set_user_config(&e, &samwise, &user_config.config);
//             storage::set_pool_config(&e, &pool_config);
//             storage::set_backstop(&e, &backstop_id);

//             b_token_0.mint(&samwise, &2);
//             d_token_1.mint(&samwise, &1);

//             e.budget().reset_unlimited();
//             let result =
//                 create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap_optimized();

//             assert_eq!(result.block, 51);
//             assert_eq!(result.bid.get_unchecked(reserve_1.config.index).unwrap_optimized(), 1);
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(result.lot.get_unchecked(reserve_0.config.index).unwrap_optimized(), 2);
//             assert_eq!(result.lot.len(), 1);
//             //scale up modifiers
//             e.ledger().set(LedgerInfo {
//                 timestamp: 12345,
//                 protocol_version: 1,
//                 sequence_number: 50 + 300,
//                 network_id: Default::default(),
//                 base_reserve: 10,
//             });
//             //liquidate user
//             let auction_data = AuctionData {
//                 bid: map![&e, (reserve_1.config.index, 0)],
//                 lot: map![&e, (reserve_0.config.index, 2)],
//                 block: 50,
//             };
//             let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
//             assert_eq!(result.bid.len(), 1);
//             assert_eq!(result.lot.len(), 1);
//             assert_eq!(result.block, 50 + 300);
//             assert_eq!(result.bid.get_unchecked(0).unwrap_optimized(), (reserve_1.asset, 0));
//             assert_eq!(
//                 result.lot.get_unchecked(0).unwrap_optimized(),
//                 (reserve_0.config.b_token, 00_0000002)
//             );
//             assert_eq!(b_token_0.balance(&frodo), 00_0000002);
//             assert_eq!(b_token_0.balance(&samwise), 00_0000000);
//             assert_eq!(d_token_1.balance(&samwise), 00_0000001);
//         });
//     }
// }
