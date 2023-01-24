use crate::{
    dependencies::TokenClient,
    errors::PoolError,
    pool::execute_repay,
    reserve::Reserve,
    storage::{AuctionData, PoolDataStore, StorageManager},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{Env, Vec};
use cast::i128;

/// ### Auction
///
/// A struct for managing auctions
pub struct Auction {
    pub auction_id: Identifier,    // the id of the auction
    pub auction_data: AuctionData, // the data for the auction
}

#[derive(Clone, PartialEq)]
#[repr(u32)]
pub enum AuctionType {
    UserLiquidation = 0,
    BackstopLiquidation = 1,
    BadDebtAuction = 2,
    AccruedInterestAuction = 3,
}

impl Auction {
    /// Load an auction
    ///
    /// ### Arguments
    /// * `auction_id` - The identifier of the auction to load
    ///
    /// ### Returns
    /// The Auction struct
    pub fn load(e: &Env, auction_id: Identifier, storage: &StorageManager) -> Auction {
        let auction_data = storage.get_auction_data(auction_id.clone());

        Auction {
            auction_id,
            auction_data,
        }
    }

    //*********** Settlement Functions **********/
    pub fn settle_asks(&self, e: &Env, invoker_id: Identifier, ask_amts: Vec<u64>) {
        for i in 0..self.auction_data.ask_count {
            let token_client =
                TokenClient::new(&e, self.auction_data.ask_ids.get(i).unwrap().unwrap());
            token_client.xfer(
                &Signature::Invoker,
                &0,
                &invoker_id,
                &i128(ask_amts.get(i).unwrap().unwrap()),
            )
        }
    }

    pub fn settle_bids(
        &self,
        e: &Env,
        from: Identifier,
        storage: &StorageManager,
        bid_amts: Vec<u64>,
    ) {
        for i in 0..self.auction_data.bid_count {
            let reserve = Reserve::load(&e, self.auction_data.bid_ids.get(i).unwrap().unwrap());
            execute_repay(
                &e,
                reserve,
                i128(bid_amts.get(i).unwrap().unwrap()),
                from.clone(),
                &self.auction_id,
                storage,
            );
        }
    }

    pub fn remove_auction(&self, storage: &StorageManager) {
        storage.remove_auction_data(self.auction_id.clone());
    }
}

pub trait AuctionManagement {
    fn load(e: &Env, auction_id: Identifier, storage: &StorageManager) -> Self;

    fn fill(
        &self,
        e: &Env,
        invoker_id: Identifier,
        storage: StorageManager,
    ) -> Result<(), PoolError>;
}

// *********** Helpers ***********

pub fn get_modified_accrued_interest(
    e: &Env,
    auction_data: AuctionData,
    ask_modifier: u64,
) -> Vec<u64> {
    let mut accrued_interest_amts: Vec<u64> = Vec::new(e);
    for id in auction_data.ask_ids.iter() {
        let asset_id = id.unwrap();
        //update reserve rate
        let mut reserve = Reserve::load(e, asset_id.clone());
        reserve.update_rates(e);
        reserve.set_data(e);
        //TODO: get backstop interest accrued from this reserve - currently not implemented
        let accrued_interest: u64 = 1_000_0000;
        //cast to u128 to avoid overflow
        accrued_interest_amts
            .push_back((accrued_interest as u128 * ask_modifier as u128 / 1_000_0000) as u64);
    }
    return accrued_interest_amts;
}

pub fn get_modified_bad_debt_amts(
    e: &Env,
    auction_data: AuctionData,
    bid_modifier: u64,
    storage: &StorageManager,
) -> Vec<u64> {
    let mut bid_amts: Vec<u64> = Vec::new(e);
    let backstop = storage.get_oracle(); //TODO: replace with method to get backstop id
    let backstop_id = Identifier::Contract(backstop);
    for id in auction_data.bid_ids.iter() {
        let asset = id.unwrap();
        // update reserve rates
        let mut reserve = Reserve::load(e, asset.clone());
        reserve.update_rates(e);
        reserve.set_data(e);
        //TODO: update when we decide how to handle dTokens
        let d_token_client = TokenClient::new(e, reserve.config.d_token.clone());
        let d_tokens = d_token_client.balance(&backstop_id);
        let underlying_debt = reserve.to_asset_from_d_token(d_tokens);
        // cast to u128 to avoid overflow
        bid_amts.push_back((underlying_debt as u128 * bid_modifier as u128 / 1_000_0000) as u64);
    }
    return bid_amts;
}

//TODO: fixed point math library
pub fn get_ask_bid_modifier(block_dif: i128) -> (u64, u64) {
    let ask_modifier: u64;
    let bid_modifier: u64;
    if block_dif > 400 {
        ask_modifier = 1_000_0000;
        bid_modifier = 0;
    } else if block_dif > 200 {
        ask_modifier = 1_000_0000;
        bid_modifier = (-block_dif / 2 * 1_0000000 / 100 + 2_0000000) as u64;
    } else {
        ask_modifier = (block_dif / 2 * 1_0000000 / 100) as u64;
        bid_modifier = 1_000_0000;
    };
    (ask_modifier, bid_modifier)
}

#[cfg(test)]
mod tests {

    use crate::{
        reserve_usage::ReserveUsage,
        storage::{ReserveConfig, ReserveData},
        testutils::{create_mock_oracle, create_token_contract, generate_contract_id},
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        vec,
    };

    #[test]
    fn test_load_auction() {
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

        // setup asset reserves
        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool_id, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &1_0000000);
        oracle_client.set_price(&asset_id_1, &1_0000000);

        // setup user
        let liability_amount = 10_0000000;
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
        d_token_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &liability_amount,
        );
        //setup auction data
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

        e.as_contract(&pool_id, || {
            storage.set_auction_data(samwise_id.clone(), data);

            let auction = Auction::load(&e, samwise_id.clone(), &storage);

            let auction_data = auction.auction_data;
            assert_eq!(auction_data.auct_type, 0);
            assert_eq!(auction_data.ask_ids, ask_ids);
            assert_eq!(auction_data.bid_ids, bid_ids);
            assert_eq!(auction_data.strt_block, 100);
            assert_eq!(auction_data.bid_count, 1);
            assert_eq!(auction_data.ask_count, 1);
            assert_eq!(auction_data.bid_ratio, 500_0000);
        });
    }

    #[test]
    fn test_get_accrued_interest() {
        //TODO: test once getting accrued interest is possible
    }

    #[test]
    fn test_get_bad_debt_amts() {
        //TODO: implement once we start tracking bad debt
    }

    #[test]
    fn test_settle_asks() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 300,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        let pool = generate_contract_id(&e);
        let pool_id = Identifier::Contract(pool.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        //setup asset 0
        let (asset_id_0, asset_0) = create_token_contract(&e, &bombadil_id);

        // setup asset 1
        let (asset_id_1, asset_1) = create_token_contract(&e, &bombadil_id);

        // setup pool
        let collateral_amount = 20_000_0000;
        asset_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &pool_id,
            &collateral_amount,
        );
        asset_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &pool_id,
            &(collateral_amount / 2),
        );

        //setup auction data
        let ask_ids = vec![&e, asset_id_0, asset_id_1.clone()];
        let bid_ids = vec![&e, asset_id_1];
        let bid_ratio: u64 = 500_0000;
        let data = AuctionData {
            auct_type: AuctionType::UserLiquidation as u32,
            ask_ids: ask_ids.clone(),
            bid_ids: bid_ids.clone(),
            strt_block: 100,
            bid_count: 1,
            ask_count: 2,
            bid_ratio,
        };

        e.as_contract(&pool, || {
            let auction = Auction {
                auction_data: data.clone(),
                auction_id: samwise_id.clone(),
            };
            let ask_amts = vec![&e, collateral_amount as u64, (collateral_amount / 2) as u64];

            auction.settle_asks(&e, samwise_id.clone(), ask_amts.clone());
            assert_eq!(asset_0.balance(&samwise_id), collateral_amount);
            assert_eq!(asset_1.balance(&samwise_id), collateral_amount / 2);
        });
    }

    #[test]
    fn test_settle_bids() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 300,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        //setup pool and users
        let pool = generate_contract_id(&e);
        let pool_id = Identifier::Contract(pool.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        //setup collateral and liabilities
        let liability_amount: i128 = 60_000_0000;
        //setup asset 0
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
        // setup reserves
        e.as_contract(&pool, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &2_000_0000);
        oracle_client.set_price(&asset_id_1, &500_0000);

        // setup user
        e.as_contract(&pool, || {
            let mut user_config = ReserveUsage::new(0);
            user_config.set_liability(0, true);
            user_config.set_liability(1, true);

            storage.set_user_config(samwise_id.clone(), user_config.config);
        }); // sets the liability as "borrowed" for the reserve at index 0
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
            let bid_amts = vec![&e, liability_amount as u64, (liability_amount / 2) as u64];

            //verify user state pre settlement
            assert_eq!(d_token_0.balance(&samwise_id), liability_amount);
            assert_eq!(d_token_1.balance(&samwise_id), liability_amount / 2);
            assert_eq!(asset_0.balance(&samwise_id), liability_amount);
            assert_eq!(asset_1.balance(&samwise_id), liability_amount / 2);
            auction.settle_bids(&e, samwise_id.clone(), &storage, bid_amts);
            //verify user state post settlement
            assert_eq!(d_token_0.balance(&samwise_id), 0);
            assert_eq!(d_token_1.balance(&samwise_id), 0);
            assert_eq!(asset_0.balance(&samwise_id), 0);
            assert_eq!(asset_1.balance(&samwise_id), 0);
        });
    }

    #[test]
    fn test_modifier_calcs() {
        let mut modifier = get_ask_bid_modifier(7);
        assert_eq!(modifier, (0_030_0000, 1_000_0000));
        modifier = get_ask_bid_modifier(250);
        assert_eq!(modifier, (1_000_0000, 750_0000));
        modifier = get_ask_bid_modifier(420);
        assert_eq!(modifier, (1_000_0000, 0));
    }
}
