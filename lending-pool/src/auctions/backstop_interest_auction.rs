use crate::{
    constants::SCALAR_7,
    dependencies::{OracleClient, TokenClient},
    errors::PoolError,
    pool::Pool,
    storage,
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, panic_with_error, unwrap::UnwrapOptimized, Address, Env};

use super::{apply_fill_modifiers, AuctionData, AuctionType};

pub fn create_interest_auction_data(e: &Env, backstop: &Address) -> AuctionData {
    if storage::has_auction(e, &(AuctionType::InterestAuction as u32), backstop) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }

    let pool = Pool::load(e);
    let oracle_client = OracleClient::new(e, &pool.config.oracle);

    let mut auction_data = AuctionData {
        lot: map![e],
        bid: map![e],
        block: e.ledger().sequence() + 1,
    };

    let reserve_list = storage::get_res_list(e);
    let mut interest_value = 0; // expressed in the oracle's decimals
    for i in 0..reserve_list.len() {
        let res_asset_address = reserve_list.get_unchecked(i);
        // don't store updated reserve data back to ledger. This will occur on the the auction's fill.
        let reserve = pool.load_reserve(e, &res_asset_address);
        if reserve.backstop_credit > 0 {
            let asset_to_base = oracle_client.get_price(&res_asset_address);
            interest_value += i128(asset_to_base)
                .fixed_mul_floor(reserve.backstop_credit, reserve.scalar)
                .unwrap_optimized();
            auction_data
                .lot
                .set(res_asset_address, reserve.backstop_credit);
        }
    }

    // Ensure that the interest value is at least 200 USDC
    if interest_value <= 200_0000000 {
        panic_with_error!(e, PoolError::InterestTooSmall);
    }

    if auction_data.lot.len() == 0 || interest_value == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let usdc_token = storage::get_usdc_token(e);
    let usdc_to_base = oracle_client.get_price(&usdc_token);
    let bid_amount = interest_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap_optimized()
        .fixed_div_floor(i128(usdc_to_base), SCALAR_7)
        .unwrap_optimized();
    // u32::MAX is the key for the USDC lot
    auction_data.bid.set(storage::get_usdc_token(e), bid_amount);

    auction_data
}

pub fn fill_interest_auction(e: &Env, auction_data: &mut AuctionData, filler: &Address) {
    apply_fill_modifiers(e, auction_data);

    // bid only contains the USDC token
    let usdc_token = storage::get_usdc_token(e);
    let bid_amount = auction_data.bid.get_unchecked(usdc_token);

    // TODO: add donate_usdc function to backstop
    // let backstop_client = BackstopClient::new(&e, &backstop_address);
    // backstop_client.donate(&filler, &e.current_contract_id(), &bid_amount_modified);

    // lot contains underlying tokens, but the backstop credit must be updated on the reserve
    let pool = Pool::load(e);
    for (res_asset_address, lot_amount) in auction_data.lot.iter() {
        let mut reserve = pool.load_reserve(e, &res_asset_address);
        reserve.backstop_credit -= lot_amount;
        reserve.store(e);
        TokenClient::new(e, &res_asset_address).transfer(
            &e.current_contract_address(),
            &filler,
            &lot_amount,
        );
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::auction::AuctionType,
        storage::{self, PoolConfig},
        testutils,
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address,
    };

    #[test]
    #[should_panic]
    //#[should_panic(expected = "ContractError(103)")]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let auction_data = AuctionData {
            bid: map![&e],
            lot: map![&e],
            block: 50,
        };
        e.as_contract(&pool_address, || {
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );

            create_interest_auction_data(&e, &backstop_address);
        });
    }

    #[test]
    #[should_panic]
    // #[should_panic(expected = "ContractError(109)")]
    fn test_create_interest_auction_under_threshold() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, _) = testutils::create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.last_time = 12345;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_data_2.b_rate = 1_100_000_000;
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);
            let mut reserve_0 = pool.load_reserve(&e, &underlying_0);
            reserve_0.backstop_credit += 10_0000000;
            reserve_0.store(&e);
            let mut reserve_1 = pool.load_reserve(&e, &underlying_1);
            reserve_1.backstop_credit += 2_5000000;
            reserve_1.store(&e);
            let result = create_interest_auction_data(&e, &backstop_address);

            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(usdc_id), 42_0000000);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.get_unchecked(underlying_0), 10_0000000);
            assert_eq!(result.lot.get_unchecked(underlying_1), 2_5000000);
            assert_eq!(result.lot.len(), 2);
        });
    }

    #[test]
    fn test_create_interest_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, _) = testutils::create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.last_time = 12345;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_data_2.b_rate = 1_100_000_000;
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);
            let mut reserve_0 = pool.load_reserve(&e, &underlying_0);
            reserve_0.backstop_credit += 100_0000000;
            reserve_0.store(&e);
            let mut reserve_1 = pool.load_reserve(&e, &underlying_1);
            reserve_1.backstop_credit += 25_0000000;
            reserve_1.store(&e);
            let result = create_interest_auction_data(&e, &backstop_address);

            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(usdc_id), 420_0000000);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.get_unchecked(underlying_0), 100_0000000);
            assert_eq!(result.lot.get_unchecked(underlying_1), 25_0000000);
            assert_eq!(result.lot.len(), 2);
        });
    }

    #[test]
    fn test_create_interest_auction_applies_interest() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 150,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, _) = testutils::create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.last_time = 11845;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_1.last_time = 11845;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_data_2.b_rate = 1_100_000_000;
        reserve_data_2.last_time = 11845;
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);

            let pool = Pool::load(&e);
            let mut reserve_0 = pool.load_reserve(&e, &underlying_0);
            reserve_0.backstop_credit += 100_0000000;
            reserve_0.store(&e);
            let mut reserve_1 = pool.load_reserve(&e, &underlying_1);
            reserve_1.backstop_credit += 25_0000000;
            reserve_1.store(&e);

            let result = create_interest_auction_data(&e, &backstop_address);
            assert_eq!(result.block, 151);
            assert_eq!(result.bid.get_unchecked(usdc_id), 420_0009794);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.get_unchecked(underlying_0), 100_0000066);
            assert_eq!(result.lot.get_unchecked(underlying_1), 25_0000066);
            assert_eq!(result.lot.get_unchecked(underlying_2), 66);
            assert_eq!(result.lot.len(), 3);
        });
    }

    #[test]
    fn test_fill_interest_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 301, // 75% bid, 100% lot
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, usdc_client) = testutils::create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.last_time = 12345;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );
        underlying_0_client.mint(&pool_address, &1_000_0000000);

        let (underlying_1, underlying_1_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        underlying_1_client.mint(&pool_address, &1_000_0000000);

        let (underlying_2, underlying_2_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta(&e);
        reserve_data_2.b_rate = 1_100_000_000;
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        underlying_2_client.mint(&pool_address, &1_000_0000000);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let mut auction_data = AuctionData {
            bid: map![&e, (usdc_id.clone(), 952_0000000)],
            lot: map![
                &e,
                (underlying_0.clone(), 100_0000000),
                (underlying_1.clone(), 25_0000000)
            ],
            block: 51,
        };
        usdc_client.mint(&samwise, &95_2000000);
        //samwise increase allowance for pool
        usdc_client.approve(&samwise, &pool_address, &i128::MAX, &1000000);
        e.as_contract(&pool_address, || {
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);
            storage::set_usdc_token(&e, &usdc_id);

            usdc_client.approve(
                &pool_address,
                &backstop_address,
                &(u64::MAX as i128),
                &1000000,
            );

            let pool = Pool::load(&e);
            let mut reserve_0 = pool.load_reserve(&e, &underlying_0);
            reserve_0.backstop_credit += 100_0000000;
            reserve_0.store(&e);
            let mut reserve_1 = pool.load_reserve(&e, &underlying_1);
            reserve_1.backstop_credit += 25_0000000;
            reserve_1.store(&e);

            e.budget().reset_unlimited();
            fill_interest_auction(&e, &mut auction_data, &samwise);
            // let result = calc_fill_interest_auction(&e, &auction);
            //TODO: test that usdc was transferred to backstop once the donate_usdc function is added to backstop
            // assert_eq!(usdc_client.balance(&samwise), 23_8000000);
            // assert_eq!(usdc_client.balance(&backstop), 71_4000000);
            assert_eq!(underlying_0_client.balance(&samwise), 100_0000000);
            assert_eq!(underlying_1_client.balance(&samwise), 25_0000000);
            let reserve_1 = pool.load_reserve(&e, &underlying_1);
            assert_eq!(reserve_1.backstop_credit, 0);
            let reserve_0 = pool.load_reserve(&e, &underlying_0);
            assert_eq!(reserve_0.backstop_credit, 0);
        });
    }
}
