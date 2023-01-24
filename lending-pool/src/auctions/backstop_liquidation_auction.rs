use crate::{
    auctions::base_auction::{
        get_ask_bid_modifier, get_modified_accrued_interest, get_modified_bad_debt_amts, Auction,
        AuctionManagement,
    },
    dependencies::BackstopClient,
    errors::PoolError,
    storage::{PoolDataStore, StorageManager},
};
use soroban_auth::Identifier;
use soroban_sdk::{vec, Env, Vec};

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
        // cast to u128 to avoid overflow
        //NOTE: think there's a bug with pop_back - TODO ask mootz
        backstop_client.draw(&ask_amts.pop_back().unwrap().unwrap(), &invoker_id.clone());
        //TODO: decide whether these are bToken transfers or not

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::println;

    use crate::{
        auctions::base_auction::AuctionType,
        reserve_usage::ReserveUsage,
        storage::{AuctionData, ReserveConfig, ReserveData},
        testutils::{
            create_backstop, create_token_contract, create_token_from_id, generate_contract_id, create_mock_pool_factory,
        },
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        BytesN,
    };
    
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

        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool);

        // mint backstop tokens to user and approve backstop
        backstop_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &400_000_0000000, // total deposit amount
        );
        backstop_token_client
            .with_source_account(&samwise)
            .incr_allow(&Signature::Invoker, &0, &backstop_id, &(u64::MAX as i128));

        // deposit into backstop module
        backstop_client
            .with_source_account(&samwise)
            .deposit(&pool, &400_000_0000000);

        e.budget().reset();

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
            //reset gas
            e.budget().reset();
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

            //reset gas
            e.budget().reset();
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
}
