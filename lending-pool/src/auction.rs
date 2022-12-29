use crate::{
    dependencies::{OracleClient, TokenClient},
    errors::AuctionError,
    reserve::Reserve,
    storage::{AuctionData, PoolDataStore, StorageManager},
    user_data::{UserAction, UserData},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{vec, Address, BytesN, Env, Vec};

/// ### Auction
///
/// A struct for managing auctions
pub struct Auction;

#[derive(Clone, PartialEq)]
#[repr(u32)]
pub enum AuctionType {
    UserLiquidation = 0,
    BackstopLiquidation = 1,
    BadDebtAuction = 2,
    AccruedInterestAuction = 3,
}

impl Auction {
    /// Initiate an Auction
    ///
    /// ### Arguments
    /// * `auct_type` - The type of auction to initiate
    /// * `bid_ids` - The identifiers of the bidders
    /// * `bid_amts` - The amounts of the bids
    /// * `ask_ids` - The identifiers of the askers
    /// * `ask_amts` - The amounts of the asks
    ///
    /// ### Notes
    /// * Auction initiation validation is carried out in the calling function
    fn initiate(
        e: Env,
        auction_id: BytesN<32>,
        auction_type: AuctionType,
        bid_ids: Vec<BytesN<32>>,
        ask_ids: Vec<BytesN<32>>,
    ) {
        let bid_count = bid_ids.len();
        let ask_count = ask_ids.len();
        let auct_type = auction_type as u32;

        let auction_data: AuctionData = AuctionData {
            strt_block: e.ledger().sequence(),
            auct_type,
            bid_count,
            bid_ids,
            ask_count,
            ask_ids,
        };
        let storage = StorageManager::new(&e);
        storage.set_auction_data(auction_id, auction_data);
    }
    /// Fill an auction
    ///
    /// ### Arguments
    /// * `auct_id` - The identifier of the auction to fill
    ///
    /// ### Errors
    /// If the auction is not in progress - storage function will throw
    ///
    /// ### Returns
    /// The bid/ask modifier the auction was filled at
    pub fn fill(e: Env, auct_id: BytesN<32>) -> u64 {
        let storage = StorageManager::new(&e);
        let auction_data = storage.get_auction_data(auct_id.clone());

        //calculate modifiers
        let start_block = auction_data.strt_block;
        let block_dif = (e.ledger().sequence() - start_block) as u64;
        let ask_modifier: u64;
        let bid_modifier: u64;
        let return_value: u64;
        if block_dif > 200 {
            ask_modifier = 1;
            bid_modifier = get_modifier(&e, block_dif);
            return_value = bid_modifier;
        } else {
            ask_modifier = get_modifier(&e, block_dif);
            bid_modifier = 1;
            return_value = ask_modifier;
        };

        //get bid/ask amounts
        let bid_amts: Vec<u64> =
            get_bid_amts(&e, &storage, auction_data.clone(), auct_id.clone()).unwrap();
        let ask_amts: Vec<u64> = get_ask_amts(
            &e,
            &storage,
            auction_data.clone(),
            auct_id.clone(),
            bid_amts.clone(),
        )
        .unwrap();
        let invoker = e.invoker();
        let invoker_id;
        let mut ask_id_iter = auction_data.ask_ids.iter();
        let mut ask_amt_iter = ask_amts.iter();
        //perform ask token transfers
        if auction_data.auct_type == 3 {
            //if accrued interest auction the ask amounts aren't paying off debt
            //TODO: deposit backstop_tokens and increase the value of backstop shares, need function for this
        } else {
            //other liquidation types involve transferring underlying or backstop tokens
            for _ in 0..auction_data.ask_count {
                let asset_id = ask_id_iter.next().unwrap().unwrap();
                let amt =
                    (ask_amt_iter.next().unwrap().unwrap() * ask_modifier / 1_000_0000) as i128;
                //execute repay on behalf of the auction_id. TODO - push repay function down so we can use it here
            }
        }
        //perform bid token transfers
        let mut bid_id_iter = auction_data.bid_ids.iter();
        let mut bid_amt_iter = bid_amts.iter();

        match invoker {
            Address::Account(account_id) => invoker_id = Identifier::Account(account_id),
            Address::Contract(bytes) => invoker_id = Identifier::Ed25519(bytes),
        }
        if auction_data.auct_type == 0 {
            //if user liquidation auction we transfer b_tokens to the auction filler
            //TODO: implement once we decide whether to use custom b_tokens or not
        } else {
            //other liquidation types involve transferring underlying or backstop tokens
            for _ in 0..auction_data.bid_count {
                let asset_id = bid_id_iter.next().unwrap().unwrap();
                let amt =
                    (bid_amt_iter.next().unwrap().unwrap() * bid_modifier / 1_000_0000) as i128;
                //TODO add check for backstop token, we probably need a unique function for transferring the backstop token
                let token_client = TokenClient::new(&e, asset_id);
                token_client.xfer(&Signature::Invoker, &0, &invoker_id, &amt)
            }
        }
        return return_value;
    }
}

// ****** Helpers *****

fn get_contract_id(e: &Env) -> Identifier {
    Identifier::Contract(e.current_contract())
}

//TODO: fixed point math library
fn get_modifier(e: &Env, block_dif: u64) -> u64 {
    if block_dif > 200 {
        return block_dif / 2 * 1_0000000 / 100 + 2_0000000;
    } else {
        return block_dif / 2 * 1_0000000 / 100;
    }
}

//TODO: fixed point math library
fn get_ask_amts(
    e: &Env,
    storage: &StorageManager,
    auction_data: AuctionData,
    auction_id: BytesN<32>,
    bid_amts: Vec<u64>,
) -> Result<Vec<u64>, AuctionError> {
    let oracle_address = storage.get_oracle();
    let oracle_client = OracleClient::new(e, oracle_address);
    return match auction_data.auct_type {
        //user liquidation
        0 => Ok(get_target_liquidation_amts(
            e,
            storage,
            auction_id,
            auction_data.bid_ids,
            auction_data.ask_ids,
            bid_amts,
            &oracle_client,
        )),
        //backstop liquidation
        1 => Ok(get_bad_debt_amts(e, auction_data.ask_ids, &storage)),
        //bad debt auction
        2 => Ok(get_bad_debt_amts(e, auction_data.ask_ids, &storage)),
        //accrued interest auction
        3 => Ok(get_target_accrued_interest_price(
            e,
            auction_data.bid_ids,
            auction_data.ask_ids,
            bid_amts,
            &oracle_client,
        )),
        4_u32..=u32::MAX => Err(AuctionError::InvalidAuctionType),
    };
}

fn get_target_liquidation_amts(
    e: &Env,
    storage: &StorageManager,
    user_id: BytesN<32>,
    bid_ids: Vec<BytesN<32>>,
    ask_ids: Vec<BytesN<32>>,
    bid_amts: Vec<u64>,
    oracle: &OracleClient,
) -> Vec<u64> {
    //calculate auction collateral factor
    let mut effective_collateral_value: u64 = 0;
    let mut raw_collateral_value: u64 = 0;
    let mut bid_id_iter = bid_ids.iter();
    let mut bid_amt_iter = bid_amts.iter();
    for _ in 0..bid_ids.len() {
        let asset_id = bid_id_iter.next().unwrap().unwrap();
        let res_config = storage.get_res_config(asset_id.clone());
        let res_data = storage.get_res_data(asset_id.clone());
        //TODO: swap for b_token_client if we end up using a custom b_token
        let asset_to_base = oracle.get_price(&asset_id);
        let b_token_balance = bid_amt_iter.next().unwrap().unwrap();
        let underlying = (b_token_balance * res_data.b_rate) / 1_000_0000;
        let base = (underlying * asset_to_base) / 1_000_0000;
        raw_collateral_value += base;
        effective_collateral_value += (base * res_config.c_factor as u64) / 1_000_0000;
    }
    let auction_collateral_factor = effective_collateral_value / raw_collateral_value;

    //get auction liability factor
    let liability_id = ask_ids.first().unwrap().unwrap();
    let res_config = storage.get_res_config(liability_id.clone());
    let auction_liability_factor = res_config.l_factor as u64;

    //get effective liability value
    let blank_asset: [u8; 32] = Default::default();
    let user_action: UserAction = UserAction {
        asset: BytesN::from_array(&e, &blank_asset),
        b_token_delta: 0,
        d_token_delta: 0,
    };
    let user_data = UserData::load(&e, &Identifier::Ed25519(user_id), &user_action);
    let effective_liability_value: u64 = user_data.e_liability_base;

    //calculate target liquidation amount
    let numerator = effective_collateral_value - effective_liability_value;
    let denominator = 1_020_0000 * 1_000_0000 / auction_liability_factor
        - 2 * auction_collateral_factor * 1_000_0000
            / (1_000_0000 + auction_collateral_factor * auction_collateral_factor / 1_000_0000);
    vec![&e, (numerator / denominator)]
}

fn get_target_accrued_interest_price(
    e: &Env,
    bid_ids: Vec<BytesN<32>>,
    ask_ids: Vec<BytesN<32>>,
    bid_amts: Vec<u64>,
    oracle: &OracleClient,
) -> Vec<u64> {
    //update rates to accrue interest on all assets
    let mut interest_value: u64 = 0;
    let mut bid_id_iter = bid_ids.iter();
    let mut bid_amt_iter = bid_amts.iter();
    for _ in 0..bid_ids.len() {
        let asset_id = bid_id_iter.next().unwrap().unwrap();
        let accrued_interest: u64 = bid_amt_iter.next().unwrap().unwrap();
        let interest_price = oracle.get_price(&asset_id);
        interest_value += (accrued_interest * interest_price) / 1_000_0000;
    }
    //TODO: get backstop LP token client and value - currently not implemented
    let lp_share_blnd_holdings: u64 = 8_000_0000;
    let lp_share_usdc_holdings: u64 = 2_000_0000;
    //TODO: get BLND token_id from somewhere - currently not implemented - then get blend price from oracle
    let blnd_id = ask_ids.first().unwrap().unwrap();
    let blnd_value = oracle.get_price(&blnd_id);
    // There's no need to get USDC price since USDC is the base asset for pricing
    let lp_share_value = lp_share_blnd_holdings * blnd_value / 1_000_0000 + lp_share_usdc_holdings;
    let target_price = 1_400_0000 * interest_value / lp_share_value;
    vec![&e, target_price]
}

fn get_bad_debt_amts(e: &Env, ask_ids: Vec<BytesN<32>>, storage: &StorageManager) -> Vec<u64> {
    let mut ask_amts: Vec<u64> = Vec::new(e);
    for ask_id in ask_ids {
        let asset_id = ask_id.unwrap();
        let res_config = storage.get_res_config(asset_id.clone());
        // TODO: get debt bad debt for this reserve, however we end up storing that
        let debt_amt: u64 = 1_000_0000;
        ask_amts.push_back(debt_amt);
    }
    return ask_amts;
}

//TODO: fixed point math library
fn get_bid_amts(
    e: &Env,
    storage: &StorageManager,
    auction_data: AuctionData,
    auction_id: BytesN<32>,
) -> Result<Vec<u64>, AuctionError> {
    let oracle_address = storage.get_oracle();
    let oracle_client = OracleClient::new(e, oracle_address);
    let temp_return: Vec<u64> = Vec::new(e);
    return match auction_data.auct_type {
        //user liquidation
        0 => Ok(get_user_collateral(
            e,
            auction_id,
            auction_data.bid_ids,
            storage,
        )),
        //backstop liquidation
        1 => Ok(get_backstop_liquidation_bids(
            e,
            storage,
            auction_data.bid_ids,
        )),
        //bad debt auction
        2 => Ok(get_accrued_interest(e, storage, auction_data.bid_ids)),
        //accrued interest auction
        3 => Ok(get_accrued_interest(e, storage, auction_data.bid_ids)),
        4_u32..=u32::MAX => Err(AuctionError::InvalidAuctionType),
    };
}

fn get_user_collateral(
    e: &Env,
    user_id: BytesN<32>,
    bid_ids: Vec<BytesN<32>>,
    storage: &StorageManager,
) -> Vec<u64> {
    let mut collateral_amounts: Vec<u64> = Vec::new(e);
    let mut collateral_value: u64 = 0;
    for bid_id in bid_ids {
        let asset_id = bid_id.unwrap();
        let res_config = storage.get_res_config(asset_id.clone());
        let res_data = storage.get_res_data(asset_id.clone());
        //TODO: swap for b_token_client if we end up using a custom b_token
        let b_token_client = TokenClient::new(e, res_config.b_token.clone());
        collateral_amounts
            .push_back(b_token_client.balance(&Identifier::Ed25519(user_id.clone())) as u64);
    }
    return collateral_amounts;
}

fn get_accrued_interest(e: &Env, storage: &StorageManager, bid_ids: Vec<BytesN<32>>) -> Vec<u64> {
    let mut accrued_interest_amts: Vec<u64> = Vec::new(e);
    for bid_id in bid_ids {
        let asset_id = bid_id.unwrap();
        let mut reserve = Reserve::load(e, asset_id.clone());
        reserve.update_rates(e);
        //TODO: get backstop interest accrued from this reserve - currently not implemented
        let accrued_interest: u64 = 1_000_0000;
        accrued_interest_amts.push_back(accrued_interest);
    }
    return accrued_interest_amts;
}

fn get_backstop_liquidation_bids(
    e: &Env,
    storage: &StorageManager,
    bid_ids: Vec<BytesN<32>>,
) -> Vec<u64> {
    let mut bid_amts: Vec<u64> = Vec::new(e);
    //TODO add backstop_module client token pull
    let backstop_lp_tokens: Vec<u64> = vec![&e, 1_000_0000];
    let accrued_interest = get_accrued_interest(e, storage, bid_ids);
    bid_amts.append(&backstop_lp_tokens);
    bid_amts.append(&accrued_interest);
    return bid_amts;
}
