use crate::{
    dependencies::{BackstopClient, OracleClient, TokenClient},
    errors::PoolError,
    pool::execute_repay,
    reserve::Reserve,
    storage::{AuctionData, PoolDataStore, StorageManager},
    user_data::{UserAction, UserData},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{vec, Env, Vec};

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
    fn settle_asks(&self, e: &Env, invoker_id: Identifier, ask_amts: Vec<u64>) {
        let mut id_iter = self.auction_data.ask_ids.iter();
        let mut amt_iter = ask_amts.iter();
        for _ in 0..self.auction_data.ask_count {
            let asset_id = id_iter.next().unwrap().unwrap();
            let amt = (amt_iter.next().unwrap().unwrap()) as i128;
            let token_client = TokenClient::new(&e, asset_id);
            token_client.xfer(&Signature::Invoker, &0, &invoker_id, &amt)
        }
    }

    fn settle_bids(&self, e: &Env, from: Identifier, storage: &StorageManager, bid_amts: Vec<u64>) {
        let mut id_iter = self.auction_data.bid_ids.iter();
        let mut amt_iter = bid_amts.iter();
        for _ in 0..self.auction_data.bid_count {
            let asset_id = id_iter.next().unwrap().unwrap();
            let amt = amt_iter.next().unwrap().unwrap();
            let reserve = Reserve::load(&e, asset_id.clone());
            execute_repay(&e, reserve, amt, from.clone(), &self.auction_id, storage);
        }
    }

    fn remove_auction(&self, storage: &StorageManager) {
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
    let mut id_iter = auction_data.ask_ids.iter();
    for _ in 0..auction_data.ask_count {
        let asset_id = id_iter.next().unwrap().unwrap();
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
    let mut liq_amt = (user_data.e_liability_base as u128 * 1_020_0000 / 1_000_0000
        - user_data.e_collateral_base as u128)
        * auction_data.bid_ratio as u128
        / 1_000_0000;
    // check if liq amount is greater than the user's liability position
    let liability = Reserve::load(e, asset.clone());
    let d_token = TokenClient::new(e, liability.config.d_token.clone());
    let d_token_balance = d_token.balance(&user_id.clone()) as u64;
    let balance = liability.to_asset_from_d_token(&d_token_balance);
    let oracle_address = storage.get_oracle();
    let oracle = OracleClient::new(e, oracle_address);
    //cast to u128 to avoid overflow
    let price = oracle.get_price(&asset) as u128;
    let value = price * balance as u128 / 1_000_0000;
    if liq_amt > value {
        liq_amt = value;
    }
    liq_amt = liq_amt * 1_000_0000 / price;
    return liq_amt as u64;
}

pub struct BackstopLiquidationAuction {
    auction: Auction,
    ask_amts: Vec<u64>,
    bid_amts: Vec<u64>,
}
impl AuctionManagement for BackstopLiquidationAuction {
    fn load(
        e: &Env,
        auction_id: Identifier,
        storage: &StorageManager,
    ) -> BackstopLiquidationAuction {
        // load auction
        let auction = Auction::load(e, auction_id, storage);

        // get modifiers
        let block_dif = (e.ledger().sequence() - auction.auction_data.strt_block.clone()) as i128;
        let (ask_modifier, bid_modifier) = get_ask_bid_modifier(block_dif);
        let storage = StorageManager::new(&e);

        // get ask amounts
        let pool = e.current_contract();
        let backstop_id = storage.get_oracle(); //TODO swap for function that gets backstop module id
        let backstop_client = BackstopClient::new(&e, backstop_id);
        let (backstop_pool_balance, _, _) = backstop_client.p_balance(&pool);
        // cast to u128 to avoid overflow
        // in backstop liquidation auctions all accrued interest is auctioned, so the ask_modifier is always 1
        let mut ask_amts =
            get_modified_accrued_interest(e, auction.auction_data.clone(), 1_000_0000);
        ask_amts.append(&vec![
            &e,
            ((backstop_pool_balance as u128 * ask_modifier as u128 / 1_000_0000) as u64),
        ]);

        // get bid amounts
        let bid_amts =
            get_modified_bad_debt_amts(e, auction.auction_data.clone(), bid_modifier, &storage);
        BackstopLiquidationAuction {
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
            .settle_bids(e, invoker_id.clone(), &storage, self.bid_amts.clone());

        //perform ask token transfers
        let backstop_id = storage.get_oracle(); //TODO swap for function that gets backstop module id
        let backstop_client = BackstopClient::new(&e, backstop_id);
        //we need to create a new ask_amt vec in order to make it mutable
        let mut ask_amts = self.ask_amts.clone();
        //cast to u128 to avoid overflow
        //NOTE: think there's a bug with pop_back - TODO ask mootz
        backstop_client.draw(
            &e.current_contract(),
            &ask_amts.pop_back().unwrap().unwrap(),
            &invoker_id.clone(),
        );
        //TODO: decide whether these are bToken transfers or not

        Ok(())
    }
}

//************* Backstop Liquidation Auction Helpers **************/
fn get_modified_accrued_interest(
    e: &Env,
    auction_data: AuctionData,
    ask_modifier: u64,
) -> Vec<u64> {
    let mut accrued_interest_amts: Vec<u64> = Vec::new(e);
    let mut id_iter = auction_data.ask_ids.iter();
    for _ in 0..auction_data.ask_count {
        let asset_id = id_iter.next().unwrap().unwrap();
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

fn get_modified_bad_debt_amts(
    e: &Env,
    auction_data: AuctionData,
    bid_modifier: u64,
    storage: &StorageManager,
) -> Vec<u64> {
    let mut bid_amts: Vec<u64> = Vec::new(e);
    let mut bid_id_iter = auction_data.bid_ids.iter();
    let backstop = storage.get_oracle(); //TODO: replace with method to get backstop id
    let backstop_id = Identifier::Contract(backstop);
    for _ in 0..auction_data.bid_count {
        let asset = bid_id_iter.next().unwrap().unwrap();
        // update reserve rates
        let mut reserve = Reserve::load(e, asset.clone());
        reserve.update_rates(e);
        reserve.set_data(e);
        //TODO: update when we decide how to handle dTokens
        let d_token_client = TokenClient::new(e, reserve.config.d_token.clone());
        let d_tokens = d_token_client.balance(&backstop_id);
        let underlying_debt = reserve.to_asset_from_d_token(&(d_tokens as u64));
        // cast to u128 to avoid overflow
        bid_amts.push_back((underlying_debt as u128 * bid_modifier as u128 / 1_000_0000) as u64);
    }
    return bid_amts;
}

pub struct BadDebtAuction {
    auction: Auction,
    ask_amts: Vec<u64>,
    bid_amts: Vec<u64>,
}

impl AuctionManagement for BadDebtAuction {
    fn load(e: &Env, auction_id: Identifier, storage: &StorageManager) -> BadDebtAuction {
        //load auction
        let auction = Auction::load(e, auction_id, storage);

        //get modifiers
        let block_dif = (e.ledger().sequence() - auction.auction_data.strt_block) as i128;
        let (ask_modifier, bid_modifier) = get_ask_bid_modifier(block_dif);
        let storage = StorageManager::new(&e);

        //get ask amounts
        let ask_amts: Vec<u64> =
            get_modified_accrued_interest(e, auction.auction_data.clone(), ask_modifier);

        //get bid amounts
        let bid_amts =
            get_modified_bad_debt_amts(e, auction.auction_data.clone(), bid_modifier, &storage);
        BadDebtAuction {
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
            .settle_bids(e, invoker_id.clone(), &storage, self.bid_amts.clone());

        //perform ask token transfers
        //TODO: decide whether these are b_token transfers or not
        Ok(())
    }
}

pub struct AccruedInterestAuction {
    auction: Auction,
    ask_amts: Vec<u64>,
    bid_amts: Vec<u64>,
}

impl AuctionManagement for AccruedInterestAuction {
    fn load(e: &Env, auction_id: Identifier, storage: &StorageManager) -> AccruedInterestAuction {
        // load auction
        let auction = Auction::load(e, auction_id, storage);

        // get modifiers
        let block_dif = (e.ledger().sequence() - auction.auction_data.strt_block.clone()) as i128;
        let (ask_modifier, bid_modifier) = get_ask_bid_modifier(block_dif);

        // get ask amounts
        let ask_amts: Vec<u64> =
            get_modified_accrued_interest(e, auction.auction_data.clone(), ask_modifier);

        // get bid amounts
        // cast to u128 to avoid overflow
        let accrued_interest_price = (get_target_accrued_interest_price(
            e,
            auction.auction_data.clone(),
            ask_amts.clone(),
            &storage,
        ) as u128
            * bid_modifier as u128
            / 1_000_0000) as u64;
        let bid_amts = vec![e, accrued_interest_price];
        AccruedInterestAuction {
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
        let backstop_id = storage.get_oracle(); //TODO swap for function that gets backstop module id
        let backstop_client = BackstopClient::new(&e, backstop_id);
        //cast to u128 to avoid overflow
        backstop_client.donate(
            &e.current_contract(),
            &self.bid_amts.first().unwrap().unwrap(),
            &invoker_id,
        );

        //perform ask token transfers
        //TODO: decide whether we transfer these as b_tokens or not
        Ok(())
    }
}

// *********** Accrued Interest Auction Helpers ***********

fn get_target_accrued_interest_price(
    e: &Env,
    auction_data: AuctionData,
    ask_amts: Vec<u64>,
    storage: &StorageManager,
) -> u64 {
    let oracle_address = storage.get_oracle();
    let oracle = OracleClient::new(e, oracle_address);
    //cast to u128 to avoid overflow
    let mut interest_value: u128 = 0;
    let mut ask_id_iter = auction_data.ask_ids.iter();
    let mut ask_amt_iter = ask_amts.iter();
    for _ in 0..ask_id_iter.len() {
        let asset_id = ask_id_iter.next().unwrap().unwrap();

        let accrued_interest: u64 = ask_amt_iter.next().unwrap().unwrap();
        let interest_price = oracle.get_price(&asset_id);
        //cast to u128 to avoid overflow
        interest_value += (accrued_interest as u128 * interest_price as u128) / 1_000_0000;
    }
    let blnd_id = auction_data.bid_ids.first().unwrap().unwrap();
    let blnd_value = oracle.get_price(&blnd_id);
    //cast to u128 to avoid overflow
    return (1_400_0000 * interest_value as u128 / blnd_value as u128) as u64;
}

// *********** Helpers ***********

//TODO: fixed point math library
fn get_ask_bid_modifier(block_dif: i128) -> (u64, u64) {
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
        storage::{ReserveConfig, ReserveData},
        testutils::{
            create_backstop, create_mock_oracle, create_token_contract, create_token_from_id,
            generate_contract_id,
        }, reserve_usage::ReserveUsage,
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        BytesN,
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
    fn test_get_accrued_interest() {
        //TODO: test once getting accrued interest is possible
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
    fn test_get_target_accrued_int_price() {
        //TODO: implement once we start accruing accrued interest
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
            d_supply: liability_amount as u64 * 4,
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
            d_supply: liability_amount as u64 * 4,
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
            d_supply: liability_amount as u64 * 4,
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
            d_supply: liability_amount as u64 * 4,
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
            user_liq_auction
                .fill(&e, samwise_id.clone(), storage)
                .unwrap();
            assert_eq!(d_token_0.balance(&samwise_id), 0);
            assert_eq!(d_token_1.balance(&samwise_id), 0);
            assert_eq!(asset_0.balance(&samwise_id), 0);
            assert_eq!(asset_1.balance(&samwise_id), 0);
        });
    }
    #[test]
    fn test_fill_backstop_liquidation_auction() {
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

        //setup backstop
        let (backstop, backstop_client) = create_backstop(&e);
        let backstop_id = Identifier::Contract(backstop.clone());
        let backstop_token_id = BytesN::from_array(&e, &[222; 32]);
        let backstop_token_client = create_token_from_id(&e, &backstop_token_id, &bombadil_id);

        // mint backstop tokens to user and approve backstop
        backstop_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &400_000_0000000, // total deposit amount
        );
        backstop_token_client.with_source_account(&samwise).incr_allow(
            &Signature::Invoker,
            &0,
            &backstop_id,
            &(u64::MAX as i128),
        );

        // deposit into backstop module
        backstop_client
            .with_source_account(&samwise)
            .deposit(&pool, &400_000_0000000);

        // setup collateral and liabilities
        let liability_amount: i128 = 20_000_0000;
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
            d_supply: liability_amount as u64 * 4,
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
            d_supply: liability_amount as u64 * 4,
            last_block: 0,
        };

        // setup contract reserves
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

        // setup pool
        let collateral_amount = 60_000_0000;
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

        //et up auction data
        let ask_ids = vec![&e, asset_id_0.clone(), asset_id_1.clone()];
        let bid_ids = vec![&e, asset_id_0, asset_id_1];
        let bid_ratio: u64 = 500_0000;
        let data = AuctionData {
            auct_type: AuctionType::UserLiquidation as u32,
            ask_ids: ask_ids.clone(),
            bid_ids: bid_ids.clone(),
            strt_block: 100,
            bid_count: 2,
            ask_count: 2,
            bid_ratio,
        };

        e.as_contract(&pool, || {
            //set backstop as oracle for now - TODO:implement backstop address storage
            storage.set_oracle(backstop);
            let auction = Auction {
                auction_data: data.clone(),
                auction_id: samwise_id.clone(),
            };
            let backstop_liq_auction = BackstopLiquidationAuction {
                auction,
                ask_amts: vec![
                    &e,
                    collateral_amount as u64,
                    (collateral_amount / 2) as u64,
                    100_000_000_0000,
                ],
                bid_amts: vec![&e, liability_amount as u64, (liability_amount / 2) as u64],
            };

            //verify user and backstop state pre fill
            assert_eq!(d_token_0.balance(&samwise_id), liability_amount);
            assert_eq!(d_token_1.balance(&samwise_id), liability_amount / 2);
            assert_eq!(asset_0.balance(&samwise_id), liability_amount);
            assert_eq!(asset_1.balance(&samwise_id), liability_amount / 2);
            let (backstop_balance, _, _) = backstop_client.p_balance(&pool);
            assert_eq!(backstop_balance, 400_000_000_0000);

            //verify user and backstop state post fill
            backstop_liq_auction
                .fill(&e, samwise_id.clone(), storage)
                .unwrap();
            assert_eq!(d_token_0.balance(&samwise_id), 0);
            assert_eq!(d_token_1.balance(&samwise_id), 0);
            // add accrued interest asset transfer checks when they're implemented
            // assert_eq!(asset_0.balance(&samwise_id), interest_amount);
            // assert_eq!(asset_1.balance(&samwise_id), interest_amount / 2);
            let (backstop_balance, _, _) = backstop_client.p_balance(&pool);
            assert_eq!(backstop_balance, 300_000_000_0000);
            assert_eq!(backstop_token_client.balance(&samwise_id), 100_000_000_0000);
        });
    }
    #[test]
    fn test_bad_debt_auction() {
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
        let liability_amount: i128 = 20_000_0000;
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
            d_supply: liability_amount as u64 * 4,
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
            d_supply: liability_amount as u64 * 4,
            last_block: 0,
        };

        // setup pool reserves
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

        // setup pool
        let collateral_amount = 40_000_0000;
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

        // setup auction data
        let ask_ids = vec![&e, asset_id_0.clone(), asset_id_1.clone()];
        let bid_ids = vec![&e, asset_id_0, asset_id_1];
        let bid_ratio: u64 = 500_0000;
        let data = AuctionData {
            auct_type: AuctionType::UserLiquidation as u32,
            ask_ids: ask_ids.clone(),
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
            let bad_debt_auction = BadDebtAuction {
                auction,
                ask_amts: vec![&e, collateral_amount as u64, (collateral_amount / 2) as u64],
                bid_amts: vec![&e, liability_amount as u64, (liability_amount / 2) as u64],
            };
            //verify user state
            assert_eq!(d_token_0.balance(&samwise_id), liability_amount);
            assert_eq!(d_token_1.balance(&samwise_id), liability_amount / 2);
            assert_eq!(asset_0.balance(&samwise_id), liability_amount);
            assert_eq!(asset_1.balance(&samwise_id), liability_amount / 2);
            //verify liquidation amount
            bad_debt_auction
                .fill(&e, samwise_id.clone(), storage)
                .unwrap();
            assert_eq!(d_token_0.balance(&samwise_id), 0);
            assert_eq!(d_token_1.balance(&samwise_id), 0);
            // TODO: verify collateral amount transfer once transfer is implemented
            // assert_eq!(asset_0.balance(&samwise_id), collateral_amount);
            // assert_eq!(asset_1.balance(&samwise_id), collateral_amount / 2);
        });
    }
    #[test]
    fn test_fill_accrued_interest_auction() {
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

        //setup backstop
        let (backstop, backstop_client) = create_backstop(&e);
        let backstop_id = Identifier::Contract(backstop.clone());
        let backstop_token_id = BytesN::from_array(&e, &[222; 32]);
        let backstop_token_client = create_token_from_id(&e, &backstop_token_id, &bombadil_id);

        // mint backstop tokens to user and approve pool
        backstop_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &4_000_0000000, // total deposit amount
        );
        backstop_token_client.with_source_account(&samwise).incr_allow(
            &Signature::Invoker,
            &0,
            &backstop_id,
            &(u64::MAX as i128),
        );

        // setup user
        e.as_contract(&pool, || {
            let user_config = ReserveUsage::new(0);

            storage.set_user_config(samwise_id.clone(), user_config.config);
        });

        //setup collateral and liabilities
        let liability_amount: i128 = 20_000_0000;

        let (asset_id_0, asset_0) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_0, _b_token_0) = create_token_contract(&e, &bombadil_id);
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
            d_supply: liability_amount as u64 * 4,
            last_block: 0,
        };

        // setup asset 1
        let (asset_id_1, asset_1) = create_token_contract(&e, &bombadil_id);
        let (b_token_id_1, _b_token_1) = create_token_contract(&e, &bombadil_id);
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
            d_supply: liability_amount as u64 * 4,
            last_block: 0,
        };

        e.as_contract(&pool, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup pool
        let interest_amount = 40_000_0000;
        asset_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &pool_id,
            &interest_amount,
        );
        asset_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &pool_id,
            &(interest_amount / 2),
        );
        let ask_ids = vec![&e, asset_id_0.clone(), asset_id_1.clone()];
        let bid_ids = vec![&e, backstop_token_id];
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
            let bid_modifier = 750_0000;
            //set backstop as oracle for now - TODO:implement backstop address storage
            storage.set_oracle(backstop);
            let auction = Auction {
                auction_data: data.clone(),
                auction_id: samwise_id.clone(),
            };
            let accrued_int_auction = AccruedInterestAuction {
                auction,
                ask_amts: vec![&e, interest_amount as u64, (interest_amount / 2) as u64],
                bid_amts: vec![&e, 4_000_000_0000 * bid_modifier / 1_000_0000],
            };

            //verify user and backstop state pre fill
            assert_eq!(backstop_token_client.balance(&samwise_id), 4_000_000_0000);
            let (backstop_balance, _, _) = backstop_client.p_balance(&pool);
            assert_eq!(backstop_balance, 0);
            accrued_int_auction
                .fill(&e, samwise_id.clone(), storage)
                .unwrap();

            //verify state post fill
            //test collateral transfers when they are implemented
            // assert_eq!(asset_0.balance(&samwise_id), collateral_amount);
            // assert_eq!(asset_1.balance(&samwise_id), collateral_amount / 2);
            let (backstop_balance, _, _) = backstop_client.p_balance(&pool);
            assert_eq!(backstop_balance, 3_000_000_0000);
            assert_eq!(backstop_token_client.balance(&samwise_id), 1_000_000_0000);
        });
    }
}
