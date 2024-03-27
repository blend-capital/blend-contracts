use crate::{
    constants::SCALAR_7, dependencies::BackstopClient, errors::PoolError, pool::Pool, storage,
};
use cast::i128;
use sep_41_token::TokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{map, panic_with_error, unwrap::UnwrapOptimized, Address, Env, Vec};

use super::{AuctionData, AuctionType};

pub fn create_interest_auction_data(
    e: &Env,
    backstop: &Address,
    assets: &Vec<Address>,
) -> AuctionData {
    if storage::has_auction(e, &(AuctionType::InterestAuction as u32), backstop) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }

    let mut pool = Pool::load(e);
    let oracle_scalar = 10i128.pow(pool.load_price_decimals(e));
    let mut auction_data = AuctionData {
        lot: map![e],
        bid: map![e],
        block: e.ledger().sequence() + 1,
    };

    let mut interest_value = 0; // expressed in the oracle's decimals
    for res_asset_address in assets.iter() {
        // don't store updated reserve data back to ledger. This will occur on the the auction's fill.
        let reserve = pool.load_reserve(e, &res_asset_address, false);
        if reserve.backstop_credit > 0 {
            let asset_to_base = pool.load_price(e, &res_asset_address);
            interest_value += i128(asset_to_base)
                .fixed_mul_floor(reserve.backstop_credit, reserve.scalar)
                .unwrap_optimized();
            auction_data
                .lot
                .set(res_asset_address, reserve.backstop_credit);
        }
    }

    // Ensure that the interest value is at least 200 USDC
    if interest_value <= (200 * 10i128.pow(pool.load_price_decimals(e))) {
        panic_with_error!(e, PoolError::InterestTooSmall);
    }

    if auction_data.lot.is_empty() || interest_value == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let backstop_client = BackstopClient::new(&e, &storage::get_backstop(e));
    let pool_backstop_data = backstop_client.pool_data(&e.current_contract_address());
    let backstop_token_value_base = (pool_backstop_data
        .usdc
        .fixed_mul_floor(oracle_scalar, SCALAR_7)
        .unwrap_optimized()
        * 5)
    .fixed_div_floor(pool_backstop_data.tokens, SCALAR_7)
    .unwrap_optimized();
    let bid_amount = interest_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap_optimized()
        .fixed_div_floor(backstop_token_value_base, SCALAR_7)
        .unwrap_optimized();
    auction_data
        .bid
        .set(backstop_client.backstop_token(), bid_amount);

    auction_data
}

pub fn fill_interest_auction(
    e: &Env,
    pool: &mut Pool,
    auction_data: &AuctionData,
    filler: &Address,
) {
    // bid only contains the Backstop token
    let backstop = storage::get_backstop(e);
    if filler.clone() == backstop {
        panic_with_error!(e, PoolError::BadRequest);
    }
    let backstop_client = BackstopClient::new(&e, &backstop);
    let backstop_token: Address = backstop_client.backstop_token();
    let backstop_token_bid_amount = auction_data.bid.get_unchecked(backstop_token);

    backstop_client.donate(
        &filler,
        &e.current_contract_address(),
        &backstop_token_bid_amount,
    );

    // lot contains underlying tokens, but the backstop credit must be updated on the reserve
    for (res_asset_address, lot_amount) in auction_data.lot.iter() {
        let mut reserve = pool.load_reserve(e, &res_asset_address, true);
        reserve.backstop_credit -= lot_amount;
        pool.cache_reserve(reserve);
        TokenClient::new(e, &res_asset_address).transfer(
            &e.current_contract_address(),
            filler,
            &lot_amount,
        );
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::auction::AuctionType,
        storage::{self, PoolConfig},
        testutils::{self, create_comet_lp_pool, create_pool},
    };

    use super::*;
    use sep_40_oracle::testutils::Asset;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, Address, Symbol,
    };

    #[test]
    #[should_panic(expected = "Error(Contract, #1212)")]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();

        let pool_address = create_pool(&e);
        let backstop_address = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
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

            create_interest_auction_data(&e, &backstop_address, &vec![&e]);
        });
    }

    #[test]
    #[should_panic]
    fn test_create_interest_auction_no_reserve() {
        let e = Env::default();

        let pool_address = create_pool(&e);
        let backstop_address = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        e.as_contract(&pool_address, || {
            create_interest_auction_data(&e, &backstop_address, &vec![&e, Address::generate(&e)]);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1215)")]
    fn test_create_interest_auction_under_threshold() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (usdc_id, _) = testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::generate(&e),
            &usdc_id,
            &Address::generate(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.last_time = 12345;
        reserve_data_0.backstop_credit = 10_0000000;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_1.last_time = 12345;
        reserve_data_1.backstop_credit = 2_5000000;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta();
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

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
                Asset::Stellar(underlying_2),
                Asset::Stellar(usdc_id.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 100_0000000, 1_0000000]);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 4,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);

            create_interest_auction_data(&e, &backstop_address, &vec![&e]);
        });
    }

    #[test]
    fn test_create_interest_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (usdc_id, _) = testutils::create_token_contract(&e, &bombadil);
        let (blnd_id, _) = testutils::create_blnd_token(&e, &pool_address, &bombadil);

        let (backstop_token_id, _) = create_comet_lp_pool(&e, &bombadil, &blnd_id, &usdc_id);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &usdc_id,
            &blnd_id,
        );
        backstop_client.deposit(&bombadil, &pool_address, &(50 * SCALAR_7));
        backstop_client.update_tkn_val();
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.last_time = 12345;
        reserve_data_0.backstop_credit = 100_0000000;
        reserve_data_0.b_supply = 1000_0000000;
        reserve_data_0.d_supply = 750_0000000;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.last_time = 12345;
        reserve_data_1.backstop_credit = 25_0000000;
        reserve_data_1.b_supply = 250_0000000;
        reserve_data_1.d_supply = 187_5000000;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta();
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
                Asset::Stellar(underlying_2),
                Asset::Stellar(usdc_id.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 100_0000000, 1_0000000]);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 4,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);

            let result = create_interest_auction_data(
                &e,
                &backstop_address,
                &vec![&e, underlying_0.clone(), underlying_1.clone()],
            );
            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(backstop_token_id), 336_0000000);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.get_unchecked(underlying_0), 100_0000000);
            assert_eq!(result.lot.get_unchecked(underlying_1), 25_0000000);
            assert_eq!(result.lot.len(), 2);
        });
    }

    #[test]
    fn test_create_interest_auction_14_decimal_oracle() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (usdc_id, _) = testutils::create_token_contract(&e, &bombadil);
        let (blnd_id, _) = testutils::create_blnd_token(&e, &pool_address, &bombadil);

        let (backstop_token_id, _) = create_comet_lp_pool(&e, &bombadil, &blnd_id, &usdc_id);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &usdc_id,
            &blnd_id,
        );
        backstop_client.deposit(&bombadil, &pool_address, &(50 * SCALAR_7));
        backstop_client.update_tkn_val();
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.last_time = 12345;
        reserve_data_0.backstop_credit = 100_0000000;
        reserve_data_0.b_supply = 1000_0000000;
        reserve_data_0.d_supply = 750_0000000;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.last_time = 12345;
        reserve_data_1.backstop_credit = 25_0000000;
        reserve_data_1.b_supply = 250_0000000;
        reserve_data_1.d_supply = 187_5000000;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta();
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
                Asset::Stellar(underlying_2),
                Asset::Stellar(usdc_id.clone()),
            ],
            &14,
            &300,
        );
        oracle_client.set_price_stable(&vec![
            &e,
            2_0000000_0000000,
            4_0000000_0000000,
            100_0000000_0000000,
            1_0000000_0000000,
        ]);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 4,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);

            let result = create_interest_auction_data(
                &e,
                &backstop_address,
                &vec![&e, underlying_0.clone(), underlying_1.clone()],
            );
            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(backstop_token_id), 336_0000000);
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
            protocol_version: 20,
            sequence_number: 150,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (usdc_id, _) = testutils::create_token_contract(&e, &bombadil);
        let (blnd_id, _) = testutils::create_blnd_token(&e, &pool_address, &bombadil);

        let (backstop_token_id, _) = create_comet_lp_pool(&e, &bombadil, &blnd_id, &usdc_id);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &usdc_id,
            &blnd_id,
        );
        backstop_client.deposit(&bombadil, &pool_address, &(50 * SCALAR_7));
        backstop_client.update_tkn_val();

        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.last_time = 11845;
        reserve_data_0.backstop_credit = 100_0000000;
        reserve_data_0.b_supply = 1000_0000000;
        reserve_data_0.d_supply = 750_0000000;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.last_time = 11845;
        reserve_data_1.backstop_credit = 25_0000000;
        reserve_data_1.b_supply = 250_0000000;
        reserve_data_1.d_supply = 187_5000000;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta();
        reserve_data_2.last_time = 11845;
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
                Asset::Stellar(underlying_2.clone()),
                Asset::Stellar(usdc_id.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 100_0000000, 1_0000000]);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 4,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);

            let result = create_interest_auction_data(
                &e,
                &backstop_address,
                &vec![
                    &e,
                    underlying_0.clone(),
                    underlying_1.clone(),
                    underlying_2.clone(),
                ],
            );
            assert_eq!(result.block, 151);
            assert_eq!(result.bid.get_unchecked(backstop_token_id), 336_0010348);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(result.lot.get_unchecked(underlying_0), 100_0000714);
            assert_eq!(result.lot.get_unchecked(underlying_1), 25_0000178);
            assert_eq!(result.lot.get_unchecked(underlying_2), 71);
            assert_eq!(result.lot.len(), 3);
        });
    }

    #[test]
    fn test_fill_interest_auction() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 301,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (usdc_id, usdc_client) = testutils::create_token_contract(&e, &bombadil);
        let (blnd_id, blnd_client) = testutils::create_blnd_token(&e, &pool_address, &bombadil);

        let (backstop_token_id, backstop_token_client) =
            create_comet_lp_pool(&e, &bombadil, &blnd_id, &usdc_id);
        blnd_client.mint(&samwise, &10_000_0000000);
        usdc_client.mint(&samwise, &250_0000000);
        let exp_ledger = e.ledger().sequence() + 100;
        blnd_client.approve(&bombadil, &backstop_token_id, &2_000_0000000, &exp_ledger);
        usdc_client.approve(&bombadil, &backstop_token_id, &2_000_0000000, &exp_ledger);
        backstop_token_client.join_pool(
            &(100 * SCALAR_7),
            &vec![&e, 10_000_0000000, 250_0000000],
            &samwise,
        );
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &usdc_id,
            &blnd_id,
        );
        backstop_client.deposit(&bombadil, &pool_address, &(50 * SCALAR_7));
        backstop_client.update_tkn_val();

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.b_supply = 200_000_0000000;
        reserve_data_0.d_supply = 100_000_0000000;
        reserve_data_0.last_time = 12345;
        reserve_data_0.backstop_credit = 100_0000000;
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_0.b_supply = 10_000_0000000;
        reserve_data_0.b_supply = 7_000_0000000;
        reserve_data_1.last_time = 12345;
        reserve_data_1.backstop_credit = 30_0000000;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        underlying_1_client.mint(&pool_address, &1_000_0000000);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 4,
        };
        let mut auction_data = AuctionData {
            bid: map![&e, (backstop_token_id.clone(), 75_0000000)],
            lot: map![
                &e,
                (underlying_0.clone(), 100_0000000),
                (underlying_1.clone(), 25_0000000)
            ],
            block: 51,
        };
        e.as_contract(&pool_address, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);
            let mut pool = Pool::load(&e);
            let backstop_token_balance_pre_fill = backstop_token_client.balance(&backstop_address);
            fill_interest_auction(&e, &mut pool, &mut auction_data, &samwise);
            pool.store_cached_reserves(&e);

            assert_eq!(backstop_token_client.balance(&samwise), 25_0000000);
            assert_eq!(
                backstop_token_client.balance(&backstop_address),
                backstop_token_balance_pre_fill + 75_0000000
            );
            assert_eq!(underlying_0_client.balance(&samwise), 100_0000000);
            assert_eq!(underlying_1_client.balance(&samwise), 25_0000000);
            // verify only filled backstop credits get deducted from total
            let reserve_0_data = storage::get_res_data(&e, &underlying_0);
            assert_eq!(reserve_0_data.backstop_credit, 0);
            let reserve_1_data = storage::get_res_data(&e, &underlying_1);
            assert_eq!(reserve_1_data.backstop_credit, 5_0000000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1200)")]
    fn test_fill_interest_auction_with_backstop() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 301,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (usdc_id, usdc_client) = testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::generate(&e),
            &usdc_id,
            &Address::generate(&e),
        );

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta();
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        underlying_1_client.mint(&pool_address, &1_000_0000000);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 4,
        };
        let mut auction_data = AuctionData {
            bid: map![&e, (usdc_id.clone(), 95_0000000)],
            lot: map![
                &e,
                (underlying_0.clone(), 100_0000000),
                (underlying_1.clone(), 25_0000000)
            ],
            block: 51,
        };
        usdc_client.mint(&samwise, &100_0000000);
        e.as_contract(&pool_address, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);

            let mut pool = Pool::load(&e);
            fill_interest_auction(&e, &mut pool, &mut auction_data, &backstop_address);
        });
    }
}
