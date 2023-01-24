use cast::i128;
use soroban_auth::Identifier;
use soroban_sdk::{vec, Env, Vec};

use crate::auctions::base_auction::{get_ask_bid_modifier, Auction, AuctionManagement};
use crate::{
    dependencies::{OracleClient, TokenClient},
    errors::PoolError,
    reserve::Reserve,
    storage::{AuctionData, PoolDataStore, StorageManager},
    user_data::{UserAction, UserData},
};

pub struct UserLiquidationAuction {
    auction: Auction,
    ask_amts: Vec<u64>,
    bid_amts: Vec<u64>,
}
impl AuctionManagement for UserLiquidationAuction {
    fn load(e: &Env, auction_id: Identifier, storage: &StorageManager) -> UserLiquidationAuction {
        // load auction
        let auction = Auction::load(e, auction_id, storage);

        // get modifiers
        let block_dif = (e.ledger().sequence() - auction.auction_data.strt_block.clone()) as i128;
        let (ask_modifier, bid_modifier) = get_ask_bid_modifier(block_dif);

        // get bid amounts
        // cast to u128 to avoid overflow
        let liq_amt = (get_target_liquidation_amt(
            e,
            auction.auction_data.clone(),
            &storage,
            auction.auction_id.clone(),
        ) as u128
            * bid_modifier as u128
            / 1_000_0000) as u64;

        //if liq amt is less than 0 the user is no longer liquidatable and we can remove the auction
        if liq_amt <= 0 {
            auction.remove_auction(&storage);
            return UserLiquidationAuction {
                auction,
                ask_amts: vec![e],
                bid_amts: vec![e],
            };
        }
        let bid_amts = vec![e, liq_amt];

        // get ask amounts
        let ask_amts: Vec<u64> = get_user_collateral(
            e,
            auction.auction_data.clone(),
            &storage,
            ask_modifier,
            auction.auction_id.clone(),
        );

        UserLiquidationAuction {
            auction,
            ask_amts,
            bid_amts,
        }
    }

    fn fill(
        &self,
        e: &Env,
        invoker_id: Identifier,
        storage: StorageManager,
    ) -> Result<(), PoolError> {
        //perform bid token transfers
        self.auction
            .settle_bids(e, invoker_id, &storage, self.bid_amts.clone());
        //perform ask token transfers
        //if user liquidation auction we transfer b_tokens to the auction filler
        //TODO: implement once we decide whether to use custom b_tokens or not - either way we need a custom transfer mechanism
        self.auction.remove_auction(&storage);
        Ok(())
    }
}

//*********** User Liquidation Auction Helpers **********/
fn get_user_collateral(
    e: &Env,
    auction_data: AuctionData,
    storage: &StorageManager,
    ask_modifier: u64,
    user_id: Identifier,
) -> Vec<u64> {
    let mut collateral_amounts: Vec<u64> = Vec::new(e);
    for id in auction_data.ask_ids.iter() {
        let asset_id = id.unwrap();
        let res_config = storage.get_res_config(asset_id.clone());
        //TODO: swap for b_token_client if we end up using a custom b_token
        //TODO: we may want to store the b_token address in the auction data bid_ids, decide when we plug in the initiate_auction functions
        let b_token_client = TokenClient::new(e, res_config.b_token.clone());
        collateral_amounts.push_back(
            //cast to u128 to avoid overflow
            (b_token_client.balance(&user_id) as u128 * ask_modifier as u128 / 1_000_0000) as u64,
        );
    }
    return collateral_amounts;
}

fn get_target_liquidation_amt(
    e: &Env,
    auction_data: AuctionData,
    storage: &StorageManager,
    user_id: Identifier,
) -> u64 {
    let asset = auction_data.bid_ids.first().unwrap().unwrap();
    let user_action: UserAction = UserAction {
        asset: asset.clone(),
        b_token_delta: 0,
        d_token_delta: 0,
    };
    let user_data = UserData::load(&e, &user_id, &user_action);
    // cast to u128 to avoid overflow
    let mut liq_amt = (user_data.liability_base * 1_020_0000 / 1_000_0000
        - user_data.collateral_base)
        * i128(auction_data.bid_ratio)
        / 1_000_0000;
    // check if liq amount is greater than the user's liability position
    let liability = Reserve::load(e, asset.clone());
    let d_token = TokenClient::new(e, liability.config.d_token.clone());
    let d_token_balance = d_token.balance(&user_id);
    let balance = liability.to_asset_from_d_token(d_token_balance);
    let oracle_address = storage.get_oracle();
    let oracle = OracleClient::new(e, oracle_address);
    //cast to u128 to avoid overflow
    let price = i128(oracle.get_price(&asset));
    let value = price * balance / 1_000_0000;
    if liq_amt > value {
        liq_amt = value;
    }
    liq_amt = liq_amt * 1_000_0000 / price;
    return liq_amt as u64;
}

#[cfg(test)]
mod tests {

    use std::println;

    use crate::{
        auctions::base_auction::AuctionType,
        reserve_usage::ReserveUsage,
        storage::{ReserveConfig, ReserveData},
        testutils::{create_mock_oracle, create_token_contract, generate_contract_id},
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        vec,
    };
    #[test]
    fn test_get_user_multi_collateral() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 300,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        let pool_id = generate_contract_id(&e);
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        //setup collateral and liabilities
        let (asset_id_0, _asset_0) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_0, b_token_0) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_0, _d_token_0) = create_token_contract(&e, &bombadil_id);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: 0,
            last_block: 0,
        };

        // setup asset 1
        let (asset_id_1, _asset_1) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_1, b_token_1) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_1, _d_token_1) = create_token_contract(&e, &bombadil_id);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: 0,
            last_block: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup user
        let collateral_amount = 20_0000000;
        e.as_contract(&pool_id, || {
            storage.set_user_config(samwise_id.clone(), 0x0000000000000006)
        }); // ...01_10
        b_token_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &collateral_amount,
        );
        b_token_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &(collateral_amount / 2),
        );
        let ask_ids = vec![&e, asset_id_0.clone(), asset_id_1.clone()];
        let bid_ids = vec![&e];
        let bid_ratio: u64 = 500_0000;
        let auction_data = AuctionData {
            auct_type: AuctionType::UserLiquidation as u32,
            ask_ids: ask_ids.clone(),
            bid_ids: bid_ids.clone(),
            strt_block: 100,
            bid_count: 0,
            ask_count: 2,
            bid_ratio,
        };
        let ask_modifier = 500_0000;

        //initiate auction
        e.as_contract(&pool_id, || {
            let collateral_amts =
                get_user_collateral(&e, auction_data, &storage, ask_modifier, samwise_id.clone());
            assert_eq!(
                collateral_amts,
                vec![
                    &e,
                    (collateral_amount / 2) as u64,
                    (collateral_amount / 4) as u64
                ]
            );
        });
    }

    #[test]
    fn test_get_target_liquidation_amt() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 300,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        let pool_id = generate_contract_id(&e);
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        //setup collateral and liabilities
        let (asset_id_0, _asset_0) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_0, b_token_0) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_0, _d_token_0) = create_token_contract(&e, &bombadil_id);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: 0,
            last_block: 0,
        };

        // setup asset 1
        let (asset_id_1, _asset_1) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_1, _b_token_1) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_1, d_token_1) = create_token_contract(&e, &bombadil_id);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: 0,
            last_block: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool_id, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &2_000_0000);
        oracle_client.set_price(&asset_id_1, &500_0000);

        // setup user
        let collateral_amount = 20_000_0000;
        let liability_amount = 30_000_0000;
        e.as_contract(&pool_id, || {
            storage.set_user_config(samwise_id.clone(), 0x0000000000000006)
        }); // ...01_10
        e.as_contract(&pool_id, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(0, true);
            user_config.set_liability(1, true);
            storage.set_user_config(samwise_id.clone(), user_config.config);
        }); // sets the liability as "borrowed" for the reserve at index 0 and 1
        b_token_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &collateral_amount,
        );
        d_token_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &liability_amount,
        );
        let ask_ids = vec![&e, asset_id_0];
        let bid_ids = vec![&e, asset_id_1];
        let bid_ratio: u64 = 500_0000;
        let data = AuctionData {
            auct_type: AuctionType::UserLiquidation as u32,
            ask_ids: ask_ids.clone(),
            bid_ids: bid_ids.clone(),
            strt_block: 100,
            bid_count: 1,
            ask_count: 1,
            bid_ratio,
        };

        //initiate auction
        e.as_contract(&pool_id, || {
            //verify liquidation amount
            let liq_amt = get_target_liquidation_amt(&e, data.clone(), &storage, samwise_id);

            assert_eq!(liq_amt, 10_600_0000);
        });
    }

    #[test]
    fn test_get_target_liquidation_amt_pulldown() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 300,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        let pool_id = generate_contract_id(&e);
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        //setup collateral and liabilities
        // setup asset 0
        let (asset_id_0, _asset_0) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_0, b_token_0) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_0, d_token_0) = create_token_contract(&e, &bombadil_id);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: 0,
            last_block: 0,
        };

        // setup asset 1
        let (asset_id_1, _asset_1) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_1, _b_token_1) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_1, d_token_1) = create_token_contract(&e, &bombadil_id);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: 0,
            last_block: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool_id, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &2_000_0000);
        oracle_client.set_price(&asset_id_1, &500_0000);

        // setup user
        let collateral_amount = 20_000_0000;
        let liability_amount = 60_000_0000;
        e.as_contract(&pool_id, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_liability(0, true);
            user_config.set_liability(1, true);

            storage.set_user_config(samwise_id.clone(), user_config.config);
        }); // sets the liability as "borrowed" for the reserve at index 0 and 1
        b_token_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &collateral_amount,
        );
        d_token_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &(liability_amount / 3),
        );
        d_token_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &liability_amount,
        );
        //setup auction data
        let ask_ids = vec![&e, asset_id_0.clone()];
        let bid_ids = vec![&e, asset_id_0];
        let bid_ratio: u64 = 500_0000;
        let data = AuctionData {
            auct_type: AuctionType::UserLiquidation as u32,
            ask_ids: ask_ids.clone(),
            bid_ids: bid_ids.clone(),
            strt_block: 100,
            bid_count: 1,
            ask_count: 1,
            bid_ratio,
        };

        e.as_contract(&pool_id, || {
            //verify liquidation amount
            let liq_amt = get_target_liquidation_amt(&e, data, &storage, samwise_id);

            assert_eq!(liq_amt, 20_000_0000);
        });
    }
    #[test]
    fn test_fill_user_liquidation_auction() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 300,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        // setup pool and users
        let pool = generate_contract_id(&e);
        let pool_id = Identifier::Contract(pool.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        //setup collateral and liabilities
        let liability_amount: i128 = 60_000_0000;
        // setup asset 0
        let (asset_id_0, asset_0) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_0, _b_token_0) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_0, d_token_0) = create_token_contract(&e, &bombadil_id);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: liability_amount * 4,
            last_block: 0,
        };

        // setup asset 1
        let (asset_id_1, asset_1) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_1, _b_token_1) = create_token_contract(&e, &bombadil_id);
        let (d_token_id_1, d_token_1) = create_token_contract(&e, &bombadil_id);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_5000000,
            l_factor: 0_5000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            reactivity: 100,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 0,
            b_supply: 0,
            d_supply: liability_amount * 4,
            last_block: 0,
        };

        // setup contracct reserves
        e.as_contract(&pool, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup user
        e.as_contract(&pool, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_liability(0, true);
            user_config.set_liability(1, true);

            storage.set_user_config(samwise_id.clone(), user_config.config);
        }); // sets the liability as "borrowed" for the reserve at index 0 and 1
        d_token_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &liability_amount,
        );
        d_token_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &(liability_amount / 2),
        );
        asset_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &liability_amount,
        );
        asset_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &(liability_amount / 2),
        );
        asset_0.with_source_account(&samwise).incr_allow(
            &Signature::Invoker,
            &0,
            &pool_id,
            &liability_amount,
        );
        asset_1.with_source_account(&samwise).incr_allow(
            &Signature::Invoker,
            &0,
            &pool_id,
            &(liability_amount / 2),
        );
        d_token_0
            .with_source_account(&bombadil)
            .set_admin(&Signature::Invoker, &0, &pool_id);
        d_token_1
            .with_source_account(&bombadil)
            .set_admin(&Signature::Invoker, &0, &pool_id);

        // setup auction data
        let bid_ids = vec![&e, asset_id_0, asset_id_1];
        let bid_ratio: u64 = 500_0000;
        let data = AuctionData {
            auct_type: AuctionType::UserLiquidation as u32,
            ask_ids: vec![&e],
            bid_ids: bid_ids.clone(),
            strt_block: 100,
            bid_count: 2,
            ask_count: 2,
            bid_ratio,
        };

        e.as_contract(&pool, || {
            let auction = Auction {
                auction_data: data.clone(),
                auction_id: samwise_id.clone(),
            };
            let user_liq_auction = UserLiquidationAuction {
                auction,
                ask_amts: vec![&e],
                bid_amts: vec![&e, liability_amount as u64, (liability_amount / 2) as u64],
            };
            // verify user state pre fill
            assert_eq!(d_token_0.balance(&samwise_id), liability_amount);
            assert_eq!(d_token_1.balance(&samwise_id), liability_amount / 2);
            assert_eq!(asset_0.balance(&samwise_id), liability_amount);
            assert_eq!(asset_1.balance(&samwise_id), liability_amount / 2);
            // verify user state post fill
            println!("gas used: {}", e.budget());

            user_liq_auction
                .fill(&e, samwise_id.clone(), storage)
                .unwrap();
            println!("gas used: {}", e.budget());

            assert_eq!(d_token_0.balance(&samwise_id), 0);
            assert_eq!(d_token_1.balance(&samwise_id), 0);
            assert_eq!(asset_0.balance(&samwise_id), 0);
            assert_eq!(asset_1.balance(&samwise_id), 0);
        });
    }
}
