use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, vec, Address, Env};

use crate::auctions::auction::AuctionData;
use crate::constants::SCALAR_7;
use crate::pool;
use crate::reserve_usage::ReserveUsage;
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
                liq_data
                    .collateral
                    .remove_unchecked(res_asset_address.clone());
                let to_sell_amt_base = to_sell_amt
                    .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                    .unwrap();
                sell_collat_base += to_sell_amt_base;

                scaled_cf += to_sell_amt_base
                    .fixed_mul_floor(i128(reserve.config.c_factor), SCALAR_7)
                    .unwrap();
                let to_sell_b_tokens = reserve.to_b_token(e, to_sell_amt);
                if to_sell_b_tokens > b_token_balance {
                    return Err(PoolError::InvalidLiquidation);
                }
                liquidation_quote
                    .lot
                    .set(reserve.config.index, to_sell_b_tokens);
            }
        }

        if user_config.is_liability(i) {
            // append users effective liability to liability_base
            let d_token_client = TokenClient::new(e, &reserve.config.d_token);
            let d_token_balance = d_token_client.balance(user);
            let asset_liability = reserve.to_effective_asset_from_d_token(d_token_balance);
            liability_base += asset_liability
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();

            if let Some(to_buy_entry) = liq_data.liability.get(res_asset_address.clone()) {
                let to_buy_amt = to_buy_entry.unwrap();
                liq_data
                    .liability
                    .remove_unchecked(res_asset_address.clone());
                let to_buy_amt_base = to_buy_amt
                    .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                    .unwrap();
                buy_liab_base += to_buy_amt_base;

                scaled_lf += to_buy_amt_base
                    .fixed_mul_floor(i128(reserve.config.l_factor), SCALAR_7)
                    .unwrap();
                let to_buy_d_tokens = reserve.to_d_token(to_buy_amt);
                if to_buy_d_tokens > d_token_balance {
                    return Err(PoolError::InvalidLiquidation);
                }
                liquidation_quote
                    .bid
                    .set(reserve.config.index, to_buy_d_tokens);
            }
        }
    }

    // any remaining entries in liquidation data represent tokens that the user does not have
    if liq_data.collateral.len() > 0 || liq_data.liability.len() > 0 {
        return Err(PoolError::InvalidLiquidation);
    }

    if collateral_base > liability_base {
        return Err(PoolError::BadRequest);
    }

    // ensure liquidation size is fair and the collateral is large enough to allow for the auction to price the liquidation
    let weighted_cf = scaled_cf
        .fixed_div_floor(sell_collat_base, SCALAR_7)
        .unwrap();
    let weighted_lf = scaled_lf.fixed_div_floor(buy_liab_base, SCALAR_7).unwrap();
    let hf_ratio = SCALAR_7.fixed_div_floor(weighted_lf, SCALAR_7).unwrap() - weighted_cf;
    let target_liabilities = (liability_base.fixed_mul_floor(1_0300000, SCALAR_7).unwrap()
        - collateral_base)
        .fixed_div_floor(hf_ratio, SCALAR_7)
        .unwrap();
    if target_liabilities < buy_liab_base {
        return Err(PoolError::InvalidLiquidation);
    }
    if sell_collat_base
        < target_liabilities
            .fixed_mul_floor(1_2500000, SCALAR_7)
            .unwrap()
        || sell_collat_base
            > target_liabilities
                .fixed_mul_floor(1_5000000, SCALAR_7)
                .unwrap()
    {
        return Err(PoolError::InvalidLiquidation);
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
            b_token_client.clawback(&e.current_contract_address(), &user, &mod_lot_amount);
            b_token_client.mint(&e.current_contract_address(), &filler, &mod_lot_amount);
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

            pool::execute_repay(e, filler, &res_asset_address, mod_bid_amount, &user).unwrap();

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
        testutils::{create_mock_oracle, create_reserve, generate_contract_id, setup_reserve},
    };

    use super::*;
    use soroban_sdk::testutils::{Address as AddressTestTrait, Ledger, LedgerInfo};

    #[test]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);
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
        e.as_contract(&pool_id, || {
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

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_id = generate_contract_id(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 20_0000000)],
            liability: map![&e, (reserve_2.asset, 0_5000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&pool, &samwise, &90_9100000);
            b_token_1.mint(&pool, &samwise, &04_5800000);
            d_token_2.mint(&pool, &samwise, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(
                result.bid.get_unchecked(reserve_2.config.index).unwrap(),
                0_5000000
            );
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap(),
                18_1818181
            );
            assert_eq!(result.lot.len(), 1);
        });
    }

    #[test]
    fn test_create_user_liquidation_auction_too_much_collateral() {
        let e = Env::default();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_id = generate_contract_id(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![
                &e,
                (reserve_0.asset, 20_0000000),
                (reserve_1.asset, 4_5000000)
            ],
            liability: map![&e, (reserve_2.asset, 0_5000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&pool, &samwise, &90_9100000);
            b_token_1.mint(&pool, &samwise, &04_5800000);
            d_token_2.mint(&pool, &samwise, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLiquidation),
            };
        });
    }

    #[test]
    fn test_create_user_liquidation_auction_too_little_collateral() {
        let e = Env::default();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_id = generate_contract_id(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 15_0000000)],
            liability: map![&e, (reserve_2.asset, 0_5000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&pool, &samwise, &90_9100000);
            b_token_1.mint(&pool, &samwise, &04_5800000);
            d_token_2.mint(&pool, &samwise, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLiquidation),
            };
        });
    }

    #[test]
    fn test_create_user_liquidation_auction_too_large() {
        let e = Env::default();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_id = generate_contract_id(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e, (reserve_0.asset, 20_0000000)],
            liability: map![&e, (reserve_2.asset, 0_6000000)],
        };
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&pool, &samwise, &90_9100000);
            b_token_1.mint(&pool, &samwise, &04_5800000);
            d_token_2.mint(&pool, &samwise, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction_data(&e, &samwise, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLiquidation),
            };
        });
    }

    #[test]
    fn test_fill_user_liquidation_auction() {
        let e = Env::default();

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

        let pool_id = generate_contract_id(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 175;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 175;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 175;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let reserve_2_asset = TokenClient::new(&e, &reserve_2.asset);
        reserve_2_asset.mint(&bombadil, &frodo, &0_5000000);
        reserve_2_asset.incr_allow(&frodo, &pool, &i128::MAX);

        let auction_data = AuctionData {
            bid: map![&e, (reserve_2.config.index, 0_5000000)],
            lot: map![&e, (reserve_0.config.index, 18_1818181)],
            block: 50,
        };
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage::set_user_config(&e, &samwise, &user_config.config);
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&pool, &samwise, &90_9100000);
            b_token_1.mint(&pool, &samwise, &04_5800000);
            d_token_2.mint(&pool, &samwise, &02_7500000);
            let res_2_init_pool_bal = reserve_2_asset.balance(&pool);

            e.budget().reset();
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
                reserve_2_asset.balance(&pool),
                res_2_init_pool_bal + 0_5000000
            );
            assert_eq!(b_token_0.balance(&frodo), 11_3636363);
            assert_eq!(b_token_0.balance(&samwise), 79_5463637);
        });
    }
}
