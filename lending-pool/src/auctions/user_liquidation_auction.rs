use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{map, vec, Env};

use crate::auctions::auction_v2::AuctionDataV2;
use crate::constants::SCALAR_7;
use crate::pool::execute_repay;
use crate::reserve_usage::ReserveUsage;
use crate::{
    dependencies::{OracleClient, TokenClient},
    errors::PoolError,
    reserve::Reserve,
    storage::{PoolDataStore, StorageManager},
};

use super::auction_v2::{AuctionQuote, AuctionType, AuctionV2, LiquidationMetadata};

pub fn create_user_liq_auction(
    e: &Env,
    user: &Identifier,
    mut liq_data: LiquidationMetadata,
) -> Result<AuctionV2, PoolError> {
    let storage = StorageManager::new(e);

    if storage.has_auction(AuctionType::UserLiquidation as u32, user.clone()) {
        return Err(PoolError::AlreadyInProgress);
    }

    let pool_config = storage.get_pool_config();
    let oracle_client = OracleClient::new(e, pool_config.oracle.clone());

    let mut liquidation_quote = AuctionDataV2 {
        bid: map![e],
        lot: map![e],
        block: e.ledger().sequence() + 1,
    };

    let user_config = ReserveUsage::new(storage.get_user_config(user.clone()));
    let reserve_count = storage.get_res_list();
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
            let b_token_client = TokenClient::new(e, reserve.config.b_token.clone());
            let b_token_balance = b_token_client.balance(user);
            let asset_collateral = reserve.to_effective_asset_from_b_token(b_token_balance);
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
                let to_sell_b_tokens = reserve.to_b_token(to_sell_amt);
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
            let d_token_client = TokenClient::new(e, reserve.config.d_token.clone());
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

    Ok(AuctionV2 {
        auction_type: AuctionType::UserLiquidation,
        user: user.clone(),
        data: liquidation_quote,
    })
}

pub fn calc_fill_user_liq_auction(e: &Env, auction: &AuctionV2) -> AuctionQuote {
    let storage = StorageManager::new(e);

    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = auction.get_fill_modifiers(e);
    let reserve_list = storage.get_res_list();
    for i in 0..reserve_list.len() {
        if !(auction.data.bid.contains_key(i) || auction.data.lot.contains_key(i)) {
            continue;
        }

        let res_asset_address = reserve_list.get_unchecked(i).unwrap();
        let reserve_config = storage.get_res_config(res_asset_address.clone());

        // bids are liabilities stored as underlying
        if let Some(bid_amount_res) = auction.data.bid.get(i) {
            let mod_bid_amount = bid_amount_res
                .unwrap()
                .fixed_mul_floor(bid_modifier, SCALAR_7)
                .unwrap();
            auction_quote
                .bid
                .push_back((res_asset_address.clone(), mod_bid_amount));
        }

        // lot contains collateral stored as b_tokens
        if let Some(lot_amount_res) = auction.data.lot.get(i) {
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

pub fn fill_user_liq_auction(e: &Env, auction: &AuctionV2, filler: Identifier) -> AuctionQuote {
    let storage = StorageManager::new(e);
    let pool_config = storage.get_pool_config();

    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = auction.get_fill_modifiers(e);
    let reserve_list = storage.get_res_list();
    for i in 0..reserve_list.len() {
        if !(auction.data.bid.contains_key(i) || auction.data.lot.contains_key(i)) {
            continue;
        }

        let res_asset_address = reserve_list.get_unchecked(i).unwrap();
        let mut reserve = Reserve::load(e, res_asset_address.clone());
        let reserve_config = storage.get_res_config(res_asset_address.clone());

        // lot contains collateral stored as b_tokens
        if let Some(lot_amount_res) = auction.data.lot.get(i) {
            // short circuits rate_update if done for bid
            reserve
                .pre_action(e, &pool_config, 1, auction.user.clone())
                .unwrap();
            let mod_lot_amount = lot_amount_res
                .unwrap()
                .fixed_mul_floor(lot_modifier, SCALAR_7)
                .unwrap();
            auction_quote
                .lot
                .push_back((reserve_config.b_token.clone(), mod_lot_amount));

            // TODO: Privileged xfer
            let b_token_client = TokenClient::new(e, reserve.config.b_token.clone());
            b_token_client.clawback(&Signature::Invoker, &0, &auction.user, &mod_lot_amount);
            b_token_client.mint(&Signature::Invoker, &0, &filler, &mod_lot_amount);
        }

        // bids are liabilities stored as underlying
        if let Some(bid_amount_res) = auction.data.bid.get(i) {
            reserve
                .pre_action(e, &pool_config, 0, auction.user.clone())
                .unwrap();
            let mod_bid_amount = bid_amount_res
                .unwrap()
                .fixed_mul_floor(bid_modifier, SCALAR_7)
                .unwrap();
            auction_quote
                .bid
                .push_back((res_asset_address.clone(), mod_bid_amount));

            execute_repay(e, reserve, mod_bid_amount, filler.clone(), &auction.user);
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
        auctions::auction_v2::AuctionType,
        storage::{PoolConfig, PoolDataStore, StorageManager},
        testutils::{create_mock_oracle, create_reserve, generate_contract_id, setup_reserve},
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::testutils::{Accounts, Ledger, LedgerInfo};

    #[test]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);
        let (oracle, _) = create_mock_oracle(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let liquidation_data = LiquidationMetadata {
            collateral: map![&e],
            liability: map![&e],
        };

        let auction_data = AuctionDataV2 {
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
            storage.set_pool_config(pool_config);
            storage.set_auction(
                AuctionType::UserLiquidation as u32,
                samwise_id.clone(),
                auction_data,
            );

            let result = create_user_liq_auction(&e, &samwise_id, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AlreadyInProgress),
            };
        });
    }

    #[test]
    fn test_create_user_liquidation_auction() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let pool = generate_contract_id(&e);
        let (oracle, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
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
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage.set_user_config(samwise_id.clone(), user_config.config);
            storage.set_pool_config(pool_config);

            b_token_0.mint(&Signature::Invoker, &0, &samwise_id, &90_9100000);
            b_token_1.mint(&Signature::Invoker, &0, &samwise_id, &04_5800000);
            d_token_2.mint(&Signature::Invoker, &0, &samwise_id, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction(&e, &samwise_id, liquidation_data).unwrap();

            assert_eq!(
                result.auction_type as u32,
                AuctionType::UserLiquidation as u32
            );
            assert_eq!(result.user, samwise_id);
            assert_eq!(result.data.block, 51);
            assert_eq!(
                result
                    .data
                    .bid
                    .get_unchecked(reserve_2.config.index)
                    .unwrap(),
                0_5000000
            );
            assert_eq!(result.data.bid.len(), 1);
            assert_eq!(
                result
                    .data
                    .lot
                    .get_unchecked(reserve_0.config.index)
                    .unwrap(),
                18_1818181
            );
            assert_eq!(result.data.lot.len(), 1);
        });
    }

    #[test]
    fn test_create_user_liquidation_auction_too_much_collateral() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let pool = generate_contract_id(&e);
        let (oracle, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
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
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage.set_user_config(samwise_id.clone(), user_config.config);
            storage.set_pool_config(pool_config);

            b_token_0.mint(&Signature::Invoker, &0, &samwise_id, &90_9100000);
            b_token_1.mint(&Signature::Invoker, &0, &samwise_id, &04_5800000);
            d_token_2.mint(&Signature::Invoker, &0, &samwise_id, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction(&e, &samwise_id, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLiquidation),
            };
        });
    }

    #[test]
    fn test_create_user_liquidation_auction_too_little_collateral() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let pool = generate_contract_id(&e);
        let (oracle, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
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
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage.set_user_config(samwise_id.clone(), user_config.config);
            storage.set_pool_config(pool_config);

            b_token_0.mint(&Signature::Invoker, &0, &samwise_id, &90_9100000);
            b_token_1.mint(&Signature::Invoker, &0, &samwise_id, &04_5800000);
            d_token_2.mint(&Signature::Invoker, &0, &samwise_id, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction(&e, &samwise_id, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLiquidation),
            };
        });
    }

    #[test]
    fn test_create_user_liquidation_auction_too_large() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let pool = generate_contract_id(&e);
        let (oracle, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
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
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage.set_user_config(samwise_id.clone(), user_config.config);
            storage.set_pool_config(pool_config);

            b_token_0.mint(&Signature::Invoker, &0, &samwise_id, &90_9100000);
            b_token_1.mint(&Signature::Invoker, &0, &samwise_id, &04_5800000);
            d_token_2.mint(&Signature::Invoker, &0, &samwise_id, &02_7500000);

            e.budget().reset();
            let result = create_user_liq_auction(&e, &samwise_id, liquidation_data);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::InvalidLiquidation),
            };
        });
    }

    #[test]
    fn test_fill_user_liquidation_auction() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 175,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let frodo = e.accounts().generate_and_create();
        let frodo_id = Identifier::Account(frodo.clone());

        let pool = generate_contract_id(&e);
        let pool_id = Identifier::Contract(pool.clone());
        let (oracle, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 175;
        reserve_0.config.c_factor = 0_8500000;
        reserve_0.config.l_factor = 0_9000000;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 175;
        reserve_1.config.c_factor = 0_7500000;
        reserve_1.config.l_factor = 0_7500000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 175;
        reserve_2.config.c_factor = 0_0000000;
        reserve_2.config.l_factor = 0_7000000;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
        let d_token_2 = TokenClient::new(&e, &reserve_2.config.d_token);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &50_0000000);

        let reserve_2_asset = TokenClient::new(&e, &reserve_2.asset);
        reserve_2_asset.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &frodo_id,
            &0_5000000,
        );
        reserve_2_asset.with_source_account(&frodo).incr_allow(
            &Signature::Invoker,
            &0,
            &pool_id,
            &i128::MAX,
        );

        let auction = AuctionV2 {
            auction_type: AuctionType::UserLiquidation,
            user: samwise_id.clone(),
            data: AuctionDataV2 {
                bid: map![&e, (reserve_2.config.index, 0_5000000)],
                lot: map![&e, (reserve_0.config.index, 18_1818181)],
                block: 50,
            },
        };
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_supply(1, true);
            user_config.set_liability(2, true);
            storage.set_user_config(samwise_id.clone(), user_config.config);
            storage.set_pool_config(pool_config);

            b_token_0.mint(&Signature::Invoker, &0, &samwise_id, &90_9100000);
            b_token_1.mint(&Signature::Invoker, &0, &samwise_id, &04_5800000);
            d_token_2.mint(&Signature::Invoker, &0, &samwise_id, &02_7500000);

            e.budget().reset();
            let result = fill_user_liq_auction(&e, &auction, frodo_id.clone());

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
            assert_eq!(reserve_2_asset.balance(&frodo_id), 0);
            assert_eq!(reserve_2_asset.balance(&pool_id), 0_5000000);
            assert_eq!(b_token_0.balance(&frodo_id), 11_3636363);
            assert_eq!(b_token_0.balance(&samwise_id), 79_5463637);
        });
    }
}
