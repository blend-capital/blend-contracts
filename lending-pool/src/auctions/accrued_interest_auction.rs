use crate::{
    auctions::base_auction::{
        get_ask_bid_modifier, get_modified_accrued_interest, Auction, AuctionManagement,
    },
    constants::BLND_TOKEN,
    dependencies::{BackstopClient, OracleClient, TokenClient},
    errors::PoolError,
    storage::{AuctionData, PoolDataStore, StorageManager},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{vec, BytesN, Env, Vec};

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
        // perform bid token transfers
        let backstop_id = storage.get_oracle(); //TODO swap for function that gets backstop module id
        let backstop_client = BackstopClient::new(&e, backstop_id);

        // TODO: Make more seamless with "auth-next" by pre-authorizing the transfer taking place
        //       in the backstop client to avoid a double transfer.

        let amount: i128 = self.bid_amts.first().unwrap().unwrap() as i128;
        TokenClient::new(e, &BytesN::from_array(e, &BLND_TOKEN)).xfer_from(
            &Signature::Invoker,
            &0,
            &invoker_id,
            &Identifier::Contract(e.current_contract()),
            &amount,
        );

        backstop_client.donate(&e.current_contract(), &(amount as u64));

        // perform ask token transfers
        // TODO: decide whether we transfer these as b_tokens or not
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
    for i in 0..auction_data.ask_ids.len() {
        let interest_asset_price = oracle.get_price(&auction_data.ask_ids.get(i).unwrap().unwrap());
        //cast to u128 to avoid overflow
        interest_value +=
            (ask_amts.get(i).unwrap().unwrap() as u128 * interest_asset_price as u128) / 1_000_0000;
    }
    let blnd_id = auction_data.bid_ids.first().unwrap().unwrap();
    let blnd_value = oracle.get_price(&blnd_id);
    //cast to u128 to avoid overflow
    return (1_400_0000 * interest_value as u128 / blnd_value as u128) as u64;
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::base_auction::AuctionType,
        reserve_usage::ReserveUsage,
        storage::{AuctionData, PoolDataStore, ReserveConfig, ReserveData, StorageManager},
        testutils::{
            create_backstop, create_token_contract, create_token_from_id, generate_contract_id, create_mock_pool_factory,
        },
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        vec, BytesN,
    };

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
        let backstop_token_id = BytesN::from_array(&e, &BLND_TOKEN);
        let backstop_token_client = create_token_from_id(&e, &backstop_token_id, &bombadil_id);

        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool);

        // mint backstop tokens to user and approve pool
        backstop_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &4_000_0000000, // total deposit amount
        );
        backstop_token_client
            .with_source_account(&samwise)
            .incr_allow(&Signature::Invoker, &0, &pool_id, &(u64::MAX as i128));
        e.as_contract(&pool, || {
            backstop_token_client
                .incr_allow(&Signature::Invoker, &0, &backstop_id, &(u64::MAX as i128));
        });

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
    #[test]
    fn test_get_target_accrued_int_price() {
        //TODO: implement once we start accruing accrued interest
    }
}
