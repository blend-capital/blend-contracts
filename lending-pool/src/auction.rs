use crate::{
    dependencies::{BackstopClient, OracleClient, TokenClient},
    errors::AuctionError,
    pool::execute_repay,
    reserve::Reserve,
    storage::{AuctionData, PoolDataStore, StorageManager},
    user_data::{UserAction, UserData},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{vec, BytesN, Env, Vec};

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
    /// * `bid_ids` - The identifiers of assets the user will sell and the contract will buy
    /// * `bid_amts` - The amounts of the bids
    /// * `ask_ids` - The identifiers of assets the user will buy and the contract will sell
    /// * `ask_amts` - The amounts of the asks
    ///
    /// ### Notes
    /// * Auction initiation validation is carried out in the calling function
    fn initiate(
        e: &Env,
        auction_id: Identifier,
        auction_type: AuctionType,
        ask_ids: Vec<BytesN<32>>,
        bid_ids: Vec<BytesN<32>>,
        bid_ratio: u64,
    ) {
        let ask_count = ask_ids.len();
        let bid_count = bid_ids.len();
        let auct_type = auction_type as u32;

        let auction_data: AuctionData = AuctionData {
            strt_block: e.ledger().sequence(),
            auct_type,
            ask_count,
            ask_ids,
            bid_count,
            bid_ids,
            bid_ratio,
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
    /// The ask/bid modifier the auction was filled at
    pub fn fill(e: &Env, auct_id: Identifier) -> u64 {
        let storage = StorageManager::new(&e);
        let auction_data = storage.get_auction_data(auct_id.clone());

        //calculate modifiers
        let start_block = auction_data.strt_block;
        let block_dif = (e.ledger().sequence() - start_block) as i128;
        let bid_modifier: u64;
        let mut ask_modifier: u64;
        let return_value: u64;
        if block_dif > 200 {
            bid_modifier = 1;
            ask_modifier = get_modifier(&e, block_dif);
            return_value = ask_modifier;
        } else {
            bid_modifier = get_modifier(&e, block_dif);
            ask_modifier = 1;
            return_value = bid_modifier;
        };

        //get ask/bid amounts
        let ask_amts: Vec<u64> =
            get_ask_amts(&e, &storage, auction_data.clone(), auct_id.clone()).unwrap();
        let bid_amts: Vec<u64> = get_bid_amts(
            &e,
            &storage,
            auction_data.clone(),
            auct_id.clone(),
            ask_amts.clone(),
        )
        .unwrap();

        //perform bid token transfers
        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);
        if auction_data.auct_type == AuctionType::AccruedInterestAuction as u32 {
            //if accrued interest auction the bid amounts aren't paying off debt, they're adding to the backstop
            let backstop_id = storage.get_oracle(); //TODO swap for function that gets backstop module id
            let backstop_client = BackstopClient::new(&e, backstop_id);
            backstop_client.donate(
                &e.current_contract(),
                &(bid_amts.first().unwrap().unwrap() * bid_modifier / 1_000_0000),
                &invoker_id,
            )
        } else {
            let mut bid_id_iter = auction_data.bid_ids.iter();
            let mut bid_amt_iter = bid_amts.iter();
            for _ in 0..auction_data.bid_count {
                let asset_id = bid_id_iter.next().unwrap().unwrap();
                let amt = bid_amt_iter.next().unwrap().unwrap() * bid_modifier / 1_000_0000;
                let reserve = Reserve::load(&e, asset_id.clone());
                execute_repay(&e, reserve, amt, invoker_id.clone(), &auct_id, &storage);
            }
        }
        //perform ask token transfers
        let mut ask_id_iter = auction_data.ask_ids.iter();
        let mut ask_amt_iter = ask_amts.iter();

        if auction_data.auct_type == AuctionType::UserLiquidation as u32 {
            //if user liquidation auction we transfer b_tokens to the auction filler
            //TODO: implement once we decide whether to use custom b_tokens or not
        } else {
            if auction_data.auct_type == AuctionType::BackstopLiquidation as u32 {
                //if backstop liquidation auction we transfer backstop tokens to the auction filler without using the ask_amt iterator
                let pool = e.current_contract();
                let backstop_id = storage.get_oracle(); //TODO swap for function that gets backstop module id
                let backstop_client = BackstopClient::new(&e, backstop_id);
                let (backstop_pool_balance, _, _) = backstop_client.p_balance(&pool);
                backstop_client.draw(
                    &e.current_contract(),
                    &(backstop_pool_balance * ask_modifier / 1_000_0000),
                    &invoker_id,
                );
                //as modifier is set to 1 since in backstop liquidations all accrued interest is transferred
                ask_modifier = 1;
            }
            //other auction types involve transferring underlying to the auction filler
            for _ in 0..auction_data.ask_count {
                let asset_id = ask_id_iter.next().unwrap().unwrap();
                let amt =
                    (ask_amt_iter.next().unwrap().unwrap() * ask_modifier / 1_000_0000) as i128;
                let token_client = TokenClient::new(&e, asset_id);
                token_client.xfer(&Signature::Invoker, &0, &invoker_id, &amt)
            }
        }
        return return_value;
    }
}

// ****** Helpers *****

//TODO: fixed point math library
fn get_modifier(e: &Env, block_dif: i128) -> u64 {
    if block_dif > 400 {
        return 0;
    } else if block_dif > 200 {
        return (-block_dif / 2 * 1_0000000 / 100 + 2_0000000) as u64;
    } else {
        return (block_dif / 2 * 1_0000000 / 100) as u64;
    }
}

fn get_bid_amts(
    e: &Env,
    storage: &StorageManager,
    auction_data: AuctionData,
    auction_id: Identifier,
    ask_amts: Vec<u64>,
) -> Result<Vec<u64>, AuctionError> {
    return match auction_data.auct_type {
        //user liquidation
        0 => Ok(get_target_liquidation_amts(
            e,
            auction_id,
            auction_data.bid_ids,
            auction_data.bid_ratio,
        )),
        //backstop liquidation
        1 => Ok(get_bad_debt_amts(e, auction_data.bid_ids, &storage)),
        //bad debt auction
        2 => Ok(get_bad_debt_amts(e, auction_data.bid_ids, &storage)),
        //accrued interest auction
        3 => Ok(get_target_accrued_interest_price(
            e,
            auction_data.ask_ids,
            auction_data.bid_ids,
            ask_amts,
            storage,
        )),
        4_u32..=u32::MAX => Err(AuctionError::InvalidAuctionType),
    };
}

fn get_target_liquidation_amts(
    e: &Env,
    user_id: Identifier,
    bid_ids: Vec<BytesN<32>>,
    bid_ratio: u64,
) -> Vec<u64> {
    let user_action: UserAction = UserAction {
        asset: bid_ids.first().unwrap().unwrap(),
        b_token_delta: 0,
        d_token_delta: 0,
    };
    let user_data = UserData::load(&e, &user_id, &user_action);
    let liq_amt = (user_data.e_liability_base * 1_020_0000 / 1_000_0000
        - user_data.e_collateral_base)
        * bid_ratio
        / 1_000_0000;

    vec![&e, liq_amt]
}

fn get_target_accrued_interest_price(
    e: &Env,
    ask_ids: Vec<BytesN<32>>,
    bid_ids: Vec<BytesN<32>>,
    ask_amts: Vec<u64>,
    storage: &StorageManager,
) -> Vec<u64> {
    let oracle_address = storage.get_oracle();
    let oracle = OracleClient::new(e, oracle_address);
    //update rates to accrue interest on all assets
    let mut interest_value: u64 = 0;
    let mut ask_id_iter = ask_ids.iter();
    let mut ask_amt_iter = ask_amts.iter();
    for _ in 0..ask_ids.len() {
        let asset_id = ask_id_iter.next().unwrap().unwrap();
        let accrued_interest: u64 = ask_amt_iter.next().unwrap().unwrap();
        let interest_price = oracle.get_price(&asset_id);
        interest_value += (accrued_interest * interest_price) / 1_000_0000;
    }
    //TODO: get backstop LP token client and value - currently not implemented
    let lp_share_blnd_holdings: u64 = 8_000_0000;
    let lp_share_usdc_holdings: u64 = 2_000_0000;
    //TODO: get BLND token_id from somewhere - currently not implemented - then get blend price from oracle
    let blnd_id = bid_ids.first().unwrap().unwrap();
    let blnd_value = oracle.get_price(&blnd_id);
    // There's no need to get USDC price since USDC is the base asset for pricing
    let lp_share_value = lp_share_blnd_holdings * blnd_value / 1_000_0000 + lp_share_usdc_holdings;
    let target_price = 1_400_0000 * interest_value / lp_share_value;
    vec![&e, target_price]
}

fn get_bad_debt_amts(e: &Env, bid_ids: Vec<BytesN<32>>, storage: &StorageManager) -> Vec<u64> {
    let mut bid_amts: Vec<u64> = Vec::new(e);
    for bid_id in bid_ids {
        let asset_id = bid_id.unwrap();
        let res_config = storage.get_res_config(asset_id.clone());
        // TODO: get debt bad debt for this reserve, however we end up storing that
        let debt_amt: u64 = 1_000_0000;
        bid_amts.push_back(debt_amt);
    }
    return bid_amts;
}

fn get_ask_amts(
    e: &Env,
    storage: &StorageManager,
    auction_data: AuctionData,
    auction_id: Identifier,
) -> Result<Vec<u64>, AuctionError> {
    return match auction_data.auct_type {
        //user liquidation
        0 => Ok(get_user_collateral(
            e,
            auction_id,
            auction_data.ask_ids,
            storage,
        )),
        //backstop liquidation, bad debt auction, accrued interest auction
        1 | 2 | 3 => Ok(get_accrued_interest(e, storage, auction_data.ask_ids)),
        4_u32..=u32::MAX => Err(AuctionError::InvalidAuctionType),
    };
}

fn get_user_collateral(
    e: &Env,
    user_id: Identifier,
    ask_ids: Vec<BytesN<32>>,
    storage: &StorageManager,
) -> Vec<u64> {
    let mut collateral_amounts: Vec<u64> = Vec::new(e);
    for ask_id in ask_ids {
        let asset_id = ask_id.unwrap();
        let res_config = storage.get_res_config(asset_id.clone());
        //TODO: swap for b_token_client if we end up using a custom b_token
        let b_token_client = TokenClient::new(e, res_config.b_token.clone());
        collateral_amounts.push_back(b_token_client.balance(&user_id) as u64);
    }
    return collateral_amounts;
}

fn get_accrued_interest(e: &Env, storage: &StorageManager, ask_ids: Vec<BytesN<32>>) -> Vec<u64> {
    let mut accrued_interest_amts: Vec<u64> = Vec::new(e);
    for ask_id in ask_ids {
        let asset_id = ask_id.unwrap();
        let mut reserve = Reserve::load(e, asset_id.clone());
        reserve.update_rates(e);
        //TODO: get backstop interest accrued from this reserve - currently not implemented
        let accrued_interest: u64 = 1_000_0000;
        accrued_interest_amts.push_back(accrued_interest);
    }
    return accrued_interest_amts;
}

#[cfg(test)]
mod tests {

    use crate::testutils::generate_contract_id;

    use super::*;
    use soroban_sdk::testutils::{Accounts, Ledger, LedgerInfo};

    #[test]
    fn test_initiate_auction() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        let pool_id = generate_contract_id(&e);
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let collateral_1 = generate_contract_id(&e);
        let collateral_2 = generate_contract_id(&e);
        let liability_1 = generate_contract_id(&e);
        let ask_ids = vec![&e, collateral_1, collateral_2];
        let bid_ids = vec![&e, liability_1];
        let bid_ratio: u64 = 500_0000;

        //initiate auction
        e.as_contract(&pool_id, || {
            Auction::initiate(
                &e,
                samwise_id.clone(),
                AuctionType::UserLiquidation,
                ask_ids.clone(),
                bid_ids.clone(),
                bid_ratio.clone(),
            );

            //verify auction data
            let auction_data = storage.get_auction_data(samwise_id.clone());
            assert_eq!(auction_data.auct_type, 0);
            assert_eq!(auction_data.ask_ids, ask_ids);
            assert_eq!(auction_data.bid_ids, bid_ids);
            assert_eq!(auction_data.strt_block, 100);
            assert_eq!(auction_data.bid_count, 1);
            assert_eq!(auction_data.ask_count, 2);
            assert_eq!(auction_data.bid_ratio, 500_0000);
        });
    }

    #[test]
    fn test_modifier_calcs() {
        let e = Env::default();
        let mut modifier = get_modifier(&e, 7);
        assert_eq!(modifier, 0_030_0000);
        modifier = get_modifier(&e, 250);
        assert_eq!(modifier, 750_0000);
        modifier = get_modifier(&e, 420);
        assert_eq!(modifier, 0);
    }

    #[test]
    fn test_fill_auction() {
        //TODO implement once more things are plugged in since this is basically an integration test
    }
}
