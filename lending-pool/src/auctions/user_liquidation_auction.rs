use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, vec, Address, Env};

use crate::auctions::auction::AuctionData;
use crate::constants::{SCALAR_7, SCALAR_9};
use crate::pool;
use crate::reserve_usage::ReserveUsage;
use crate::validator::require_nonnegative;
use crate::{
    dependencies::{OracleClient, TokenClient},
    errors::PoolError,
    reserve::Reserve,
    storage,
};

use super::{get_fill_modifiers, AuctionQuote, AuctionType, LiquidationMetadata};

pub fn create_user_liq_auction_data(
    e: &Env,
    user: &Address,
    mut liq_data: LiquidationMetadata,
) -> Result<AuctionData, PoolError> {
    if storage::has_auction(e, &(AuctionType::UserLiquidation as u32), &user) {
        return Err(PoolError::AuctionInProgress);
    }

    let pool_config = storage::get_pool_config(e);
    let oracle_client = OracleClient::new(e, &pool_config.oracle);

    let mut liquidation_quote = AuctionData {
        bid: map![e],
        lot: map![e],
        block: e.ledger().sequence() + 1,
    };

    let user_config = ReserveUsage::new(storage::get_user_config(e, &user));
    let reserve_count = storage::get_res_list(e);
    let mut all_collateral = true;
    let mut all_liabilities = true;
    let mut collateral_base = 0;
    let mut sell_collat_base = 0;
    let mut scaled_cf = 0;
    let mut liability_base = 0;
    let mut buy_liab_base = 0;
    let mut scaled_lf = 0;
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();
        if !user_config.is_active_reserve(i) {
            continue;
        }

        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        // do not write rate information to chain
        reserve.update_rates(e, pool_config.bstop_rate);
        let asset_to_base = oracle_client.get_price(&res_asset_address);

        if user_config.is_collateral(i) {
            // append users effective collateral to collateral_base
            let b_token_client = TokenClient::new(e, &reserve.config.b_token);
            let b_token_balance = b_token_client.balance(user);
            let asset_collateral = reserve.to_effective_asset_from_b_token(e, b_token_balance);
            collateral_base += asset_collateral
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();
            if let Some(to_sell_entry) = liq_data.collateral.get(res_asset_address.clone()) {
                let to_sell_amt = to_sell_entry.unwrap();
                require_nonnegative(to_sell_amt)?;
                liq_data
                    .collateral
                    .remove_unchecked(res_asset_address.clone());
                let to_sell_amt_base = to_sell_amt
                    .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                    .unwrap();
                sell_collat_base += to_sell_amt_base;

                scaled_cf += to_sell_amt_base
                    .fixed_mul_floor(i128(reserve.config.c_factor) * 100, SCALAR_7)
                    .unwrap();
                let to_sell_b_tokens = reserve.to_b_token_down(e, to_sell_amt);
                if to_sell_b_tokens > b_token_balance {
                    return Err(PoolError::InvalidLot);
                } else if to_sell_b_tokens < b_token_balance {
                    all_collateral = false;
                }
                liquidation_quote
                    .lot
                    .set(reserve.config.index, to_sell_b_tokens);
            } else {
                all_collateral = false;
            }
        }

        if user_config.is_liability(i) {
            // append users effective liability to liability_base
            let d_token_client = TokenClient::new(e, &reserve.config.d_token);
            let d_token_balance = d_token_client.balance(user);
            let asset_liability = reserve.to_effective_asset_from_d_token(d_token_balance);
            liability_base += asset_liability
                .fixed_mul_ceil(i128(asset_to_base), SCALAR_7)
                .unwrap();
            if let Some(to_buy_entry) = liq_data.liability.get(res_asset_address.clone()) {
                let to_buy_amt = to_buy_entry.unwrap();
                require_nonnegative(to_buy_amt)?;
                liq_data
                    .liability
                    .remove_unchecked(res_asset_address.clone());
                let to_buy_amt_base = to_buy_amt
                    .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                    .unwrap();
                buy_liab_base += to_buy_amt_base;
                scaled_lf += to_buy_amt_base
                    .fixed_mul_floor(i128(reserve.config.l_factor) * 100, SCALAR_7)
                    .unwrap();
                let to_buy_d_tokens = reserve.to_d_token_down(to_buy_amt);
                if to_buy_d_tokens > d_token_balance {
                    return Err(PoolError::InvalidBids);
                } else if to_buy_d_tokens < d_token_balance {
                    all_liabilities = false;
                }
                liquidation_quote
                    .bid
                    .set(reserve.config.index, to_buy_d_tokens);
            } else {
                all_liabilities = false;
            }
        }
    }

    // ensure the user has less collateral than liabilities
    if liability_base < collateral_base {
        return Err(PoolError::InvalidLiquidation);
    }

    // any remaining entries in liquidation data represent tokens that the user does not have
    if liq_data.collateral.len() > 0 || liq_data.liability.len() > 0 {
        return Err(PoolError::InvalidLiquidation);
    }

    // ensure liquidation size is fair and the collateral is large enough to allow for the auction to price the liquidation
    let weighted_cf = scaled_cf
        .fixed_div_floor(sell_collat_base * 100, SCALAR_7)
        .unwrap();
    // weighted_lf factor is the inverse of the liability factor
    let weighted_lf = SCALAR_9
        .fixed_div_floor(
            scaled_lf.fixed_div_floor(buy_liab_base, SCALAR_7).unwrap(),
            SCALAR_7,
        )
        .unwrap();
    let est_incentive = (SCALAR_7 - weighted_cf.fixed_div_ceil(weighted_lf, SCALAR_7).unwrap())
        .fixed_div_ceil(2_0000000, SCALAR_7)
        .unwrap()
        + SCALAR_7;
    let max_target_liabilities = (liability_base.fixed_mul_ceil(1_0300000, SCALAR_7).unwrap()
        - collateral_base)
        .fixed_div_ceil(
            weighted_lf.fixed_mul_floor(1_0300000, SCALAR_7).unwrap()
                - weighted_cf.fixed_mul_ceil(est_incentive, SCALAR_7).unwrap(),
            SCALAR_7,
        )
        .unwrap();
    let min_target_liabilities = max_target_liabilities
        .fixed_div_ceil(1_1000000, SCALAR_7)
        .unwrap(); //TODO: Assess whether 10% is an appropriate range here

    let max_collateral_lot = buy_liab_base.fixed_mul_floor(2_5000000, SCALAR_7).unwrap();
    let min_collateral_lot = buy_liab_base.fixed_mul_floor(1_2500000, SCALAR_7).unwrap();

    if max_target_liabilities < buy_liab_base {
        return Err(PoolError::InvalidBidTooLarge);
    }
    if min_target_liabilities > buy_liab_base && all_liabilities == false {
        return Err(PoolError::InvalidBidTooSmall);
    }
    if sell_collat_base > max_collateral_lot {
        return Err(PoolError::InvalidLotTooLarge);
    }
    if sell_collat_base < min_collateral_lot && all_collateral == false {
        return Err(PoolError::InvalidLotTooSmall);
    }

    Ok(liquidation_quote)
}

pub fn calc_fill_user_liq_auction(e: &Env, auction_data: &AuctionData) -> AuctionQuote {
    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = get_fill_modifiers(e, auction_data);
    let reserve_list = storage::get_res_list(e);
    for i in 0..reserve_list.len() {
        if !(auction_data.bid.contains_key(i) || auction_data.lot.contains_key(i)) {
            continue;
        }

        let res_asset_address = reserve_list.get_unchecked(i).unwrap();
        let reserve_config = storage::get_res_config(e, &res_asset_address);

        // bids are liabilities stored as underlying
        if let Some(bid_amount_res) = auction_data.bid.get(i) {
            let mod_bid_amount = bid_amount_res
                .unwrap()
                .fixed_mul_floor(bid_modifier, SCALAR_7)
                .unwrap();
            auction_quote
                .bid
                .push_back((res_asset_address.clone(), mod_bid_amount));
        }

        // lot contains collateral stored as b_tokens
        if let Some(lot_amount_res) = auction_data.lot.get(i) {
            let mod_lot_amount = lot_amount_res
                .unwrap()
                .fixed_mul_floor(lot_modifier, SCALAR_7)
                .unwrap();
            auction_quote
                .lot
                .push_back((reserve_config.b_token, mod_lot_amount));
        }
    }

    auction_quote
}

pub fn fill_user_liq_auction(
    e: &Env,
    auction_data: &AuctionData,
    user: &Address,
    filler: &Address,
) -> AuctionQuote {
    let pool_config = storage::get_pool_config(e);

    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = get_fill_modifiers(e, auction_data);
    let reserve_list = storage::get_res_list(e);
    for i in 0..reserve_list.len() {
        if !(auction_data.bid.contains_key(i) || auction_data.lot.contains_key(i)) {
            continue;
        }

        let res_asset_address = reserve_list.get_unchecked(i).unwrap();
        let mut reserve = Reserve::load(e, res_asset_address.clone());
        let reserve_config = storage::get_res_config(e, &res_asset_address);

        // lot contains collateral stored as b_tokens
        if let Some(lot_amount_res) = auction_data.lot.get(i) {
            // short circuits rate_update if done for bid
            reserve
                .pre_action(e, &pool_config, 1, user.clone())
                .unwrap();
            let mod_lot_amount = lot_amount_res
                .unwrap()
                .fixed_mul_floor(lot_modifier, SCALAR_7)
                .unwrap();
            auction_quote
                .lot
                .push_back((reserve_config.b_token.clone(), mod_lot_amount));

            // TODO: Privileged xfer
            let b_token_client = TokenClient::new(e, &reserve.config.b_token);
            b_token_client.clawback(&user, &mod_lot_amount);
            b_token_client.mint(&filler, &mod_lot_amount);
        }

        // bids are liabilities stored as underlying
        if let Some(bid_amount_res) = auction_data.bid.get(i) {
            reserve
                .pre_action(e, &pool_config, 0, user.clone())
                .unwrap();
            let mod_bid_amount = bid_amount_res
                .unwrap()
                .fixed_mul_floor(bid_modifier, SCALAR_7)
                .unwrap();

            // bids are stored in d_token so we need to translate to underlying
            let bid_amount_underlying = reserve.to_asset_from_d_token(mod_bid_amount);

            pool::execute_repay(e, filler, &res_asset_address, bid_amount_underlying, &user)
                .unwrap();

            auction_quote
                .bid
                .push_back((res_asset_address, mod_bid_amount));
        } else {
            // execute repay sets data. Ensure data is set if only the collateral is modified
            reserve.set_data(e);
        }
    }

    auction_quote
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::auction::AuctionType,
        storage::{self, PoolConfig},
        testutils::{create_mock_oracle, create_reserve, setup_reserve},
    };

    use super::*;
    use soroban_sdk::testutils::{Address as AddressTestTrait, Ledger, LedgerInfo};

    #[test]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_address = Address::random(&e);
        let (oracle, _) = create_mock_oracle(&e);

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

            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AuctionInProgress),
            };
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
        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 22_0000000)],
            liability: map![&e, (reserve_2.asset, 0_7000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data).unwrap();
            assert_eq!(result.block, 51);
            assert_eq!(
                result.bid.get_unchecked(reserve_2.config.index).unwrap(),
                0_7000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap(),
                20_0000000
            );
            assert_eq!(result.lot.len(), 1);
        });
    }

    #[test]
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![
                &e,
                (reserve_0.asset, 26_0000000),
                (reserve_1.asset, -1_0000000)
            ],
            liability: map![&e, (reserve_2.asset, 0_7000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    PoolError::NegativeAmount => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 22_0000000),],
            liability: map![&e, (reserve_2.asset, -0_7000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    PoolError::NegativeAmount => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![
                &e,
                (reserve_0.asset, 33_0000000),
                (reserve_1.asset, 4_5000000)
            ],
            liability: map![&e, (reserve_2.asset, 0_6500000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLotTooLarge),
            };
        });
    }

    #[test]
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 15_0000000)],
            liability: map![&e, (reserve_2.asset, 0_6500000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLotTooSmall),
            };
        });
    }

    #[test]
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 32_0000000)],
            liability: map![&e, (reserve_2.asset, 1_2000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidBidTooLarge),
            };
        });
    }

    #[test]
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 17_0000000)],
            liability: map![&e, (reserve_2.asset, 0_4500000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidBidTooSmall),
            };
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let reserve_2_asset = TokenClient::new(&e, &reserve_2.asset);
        reserve_2_asset.mint(&frodo, &0_5000000);
        reserve_2_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        let auction_data = AuctionData {
            bid: map![&e, (reserve_2.config.index, 0_5000000)],
            lot: map![&e, (reserve_0.config.index, 18_1818181)],
            block: 50,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&samwise, &90_9100000);
            b_token_1.mint(&samwise, &04_5800000);
            d_token_2.mint(&samwise, &02_7500000);
            let res_2_init_pool_bal = reserve_2_asset.balance(&pool_address);

            e.budget().reset_unlimited();
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);

            assert_eq!(result.block, 175);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap(),
                (reserve_2.asset, 0_5000000)
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.b_token, 11_3636363)
            );
            assert_eq!(result.lot.len(), 1);
            assert_eq!(reserve_2_asset.balance(&frodo), 0);
            assert_eq!(
                reserve_2_asset.balance(&pool_address),
                res_2_init_pool_bal + 0_5000000
            );
            assert_eq!(b_token_0.balance(&frodo), 11_3636363);
            assert_eq!(b_token_0.balance(&samwise), 79_5463637);
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        let backstop_id = Address::random(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_000_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_000_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_0000000;
        reserve_1.config.l_factor = 0_7000000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
        let reserve_1_asset = TokenClient::new(&e, &reserve_1.asset);
        reserve_1_asset.mint(&frodo, &500_0000000_0000000);
        reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset.clone(), 3000_0000000)],
            liability: map![&e, (reserve_1.asset.clone(), 200_7500000_0000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_liability(1, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            b_token_0.mint(&samwise, &3000_0000000);
            d_token_1.mint(&samwise, &200_7500000_0000000);

            e.budget().reset_unlimited();
            let result =
                create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(
                result.bid.get_unchecked(reserve_1.config.index).unwrap(),
                200_7500000_0000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap(),
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
                bid: map![&e, (reserve_1.config.index, 200_7500000_0000000)],
                lot: map![&e, (reserve_0.config.index, 3000_0000000)],
                block: 50,
            };
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.len(), 1);
            assert_eq!(result.block, 50 + 399);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap(),
                (reserve_1.asset, 1_0037500_0000000)
            );
            assert_eq!(
                result.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.b_token, 3000_0000000)
            );
            assert_eq!(b_token_0.balance(&frodo), 3000_0000000);
            assert_eq!(
                reserve_1_asset.balance(&frodo),
                500_0000000_0000000 - 1_0037500_0000000
            );
            assert_eq!(b_token_0.balance(&samwise), 00_0000000);
            assert_eq!(
                d_token_1.balance(&samwise),
                200_7500000_0000000 - 1_0037500_0000000 + 381022054
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        let backstop_id = Address::random(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(2_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_000_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_0000000;
        reserve_1.config.l_factor = 0_7000000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
        let reserve_1_asset = TokenClient::new(&e, &reserve_1.asset);
        reserve_1_asset.mint(&frodo, &500_0000000);
        reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset.clone(), 00_0000003)],
            liability: map![&e, (reserve_1.asset.clone(), 2_7500000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_liability(1, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            b_token_0.mint(&samwise, &00_0000001);
            d_token_1.mint(&samwise, &02_7500000);

            e.budget().reset_unlimited();
            let result =
                create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(
                result.bid.get_unchecked(reserve_1.config.index).unwrap(),
                2_7500000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap(),
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
                bid: map![&e, (reserve_1.config.index, 2_7500000)],
                lot: map![&e, (reserve_0.config.index, 00_0000001)],
                block: 50,
            };
            //TODO: fix this
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.len(), 1);
            assert_eq!(result.block, 50 + 400);
            assert_eq!(result.bid.get_unchecked(0).unwrap(), (reserve_1.asset, 0));
            assert_eq!(
                result.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.b_token, 00_0000001)
            );
            assert_eq!(b_token_0.balance(&frodo), 00_0000001);
            assert_eq!(reserve_1_asset.balance(&frodo), 500_0000000);
            assert_eq!(b_token_0.balance(&samwise), 00_0000000);
            assert_eq!(d_token_1.balance(&samwise), 2_7500000);
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        let backstop_id = Address::random(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_000_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_000_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_0000000;
        reserve_1.config.l_factor = 0_7000000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
        let reserve_1_asset = TokenClient::new(&e, &reserve_1.asset);
        reserve_1_asset.mint(&frodo, &500_0000000_0000000);
        reserve_1_asset.increase_allowance(&frodo, &pool_address, &i128::MAX);

        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset.clone(), 3000_0000000)],
            liability: map![&e, (reserve_1.asset.clone(), 200_7500000_0000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_liability(1, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            b_token_0.mint(&samwise, &3000_0000000);
            d_token_1.mint(&samwise, &200_7500000_0000000);

            e.budget().reset_unlimited();
            let result =
                create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(
                result.bid.get_unchecked(reserve_1.config.index).unwrap(),
                200_7500000_0000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap(),
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
                bid: map![&e, (reserve_1.config.index, 200_7500000_0000000)],
                lot: map![&e, (reserve_0.config.index, 3000_0000000)],
                block: 50,
            };
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.len(), 1);
            assert_eq!(result.block, 50 + 399);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap(),
                (reserve_1.asset, 1_0037500_0000000)
            );
            assert_eq!(
                result.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.b_token, 3000_0000000)
            );
            assert_eq!(b_token_0.balance(&frodo), 3000_0000000);
            assert_eq!(
                reserve_1_asset.balance(&frodo),
                500_0000000_0000000 - 1_0037500_0000000
            );
            assert_eq!(b_token_0.balance(&samwise), 00_0000000);
            assert_eq!(
                d_token_1.balance(&samwise),
                200_7500000_0000000 - 1_0037500_0000000 + 381022054
            );
        });
    }
    #[test]
    fn test_liquidate_user_check_pulldown() {
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

        let (oracle_address, oracle_client) = create_mock_oracle(&e);

        let backstop_id = Address::random(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_300_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_600_000_000);
        reserve_1.data.d_rate = 2_100_000_000;
        reserve_1.data.last_time = 12345;
        reserve_1.config.c_factor = 0_1000000;
        reserve_1.config.l_factor = 0_7000000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);

        e.budget().reset_unlimited();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset.clone(), 3)],
            liability: map![&e, (reserve_1.asset.clone(), 3)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_liability(1, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            b_token_0.mint(&samwise, &2);
            d_token_1.mint(&samwise, &1);

            e.budget().reset_unlimited();
            let result =
                create_user_liq_auction_data(&e, &samwise, liquidation_data.clone()).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(reserve_1.config.index).unwrap(), 1);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.get_unchecked(reserve_0.config.index).unwrap(), 2);
            assert_eq!(result.lot.len(), 1);
            //scale up modifiers
            e.ledger().set(LedgerInfo {
                timestamp: 12345,
                protocol_version: 1,
                sequence_number: 50 + 300,
                network_id: Default::default(),
                base_reserve: 10,
            });
            //liquidate user
            let auction_data = AuctionData {
                bid: map![&e, (reserve_1.config.index, 0)],
                lot: map![&e, (reserve_0.config.index, 2)],
                block: 50,
            };
            let result = fill_user_liq_auction(&e, &auction_data, &samwise, &frodo);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.len(), 1);
            assert_eq!(result.block, 50 + 300);
            assert_eq!(result.bid.get_unchecked(0).unwrap(), (reserve_1.asset, 0));
            assert_eq!(
                result.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.b_token, 00_0000002)
            );
            assert_eq!(b_token_0.balance(&frodo), 00_0000002);
            assert_eq!(b_token_0.balance(&samwise), 00_0000000);
            assert_eq!(d_token_1.balance(&samwise), 00_0000001);
        });
    }
}
