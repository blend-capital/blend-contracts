use crate::{
    auctions::base_auction::{
        get_ask_bid_modifier, get_modified_accrued_interest, get_modified_bad_debt_amts, Auction,
        AuctionManagement,
    },
    errors::PoolError,
    storage::StorageManager,
};
use soroban_auth::Identifier;
use soroban_sdk::{Env, Vec};

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

#[cfg(test)]
mod tests {

    use crate::{
        auctions::base_auction::AuctionType,
        reserve_usage::ReserveUsage,
        storage::{AuctionData, PoolDataStore, ReserveConfig, ReserveData, StorageManager},
        testutils::{create_token_contract, generate_contract_id},
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        vec,
    };
    #[test]
    fn test_fill_bad_debt_auction() {
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
}
