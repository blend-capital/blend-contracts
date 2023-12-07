use crate::{
    constants::SCALAR_7,
    errors::PoolError,
    pool::{Pool, User},
    storage,
};
use cast::i128;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{
    contracttype, map, panic_with_error, unwrap::UnwrapOptimized, Address, Env, Map,
};

use super::{
    backstop_interest_auction::{create_interest_auction_data, fill_interest_auction},
    bad_debt_auction::{create_bad_debt_auction_data, fill_bad_debt_auction},
    user_liquidation_auction::{create_user_liq_auction_data, fill_user_liq_auction},
};

#[derive(Clone, PartialEq)]
#[repr(u32)]
pub enum AuctionType {
    UserLiquidation = 0,
    BadDebtAuction = 1,
    InterestAuction = 2,
}

impl AuctionType {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => AuctionType::UserLiquidation,
            1 => AuctionType::BadDebtAuction,
            2 => AuctionType::InterestAuction,
            _ => panic!("internal error"),
        }
    }
}

#[derive(Clone)]
#[contracttype]
pub struct AuctionData {
    pub bid: Map<Address, i128>,
    pub lot: Map<Address, i128>,
    pub block: u32,
}

/// Create an auction. Stores the resulting auction to the ledger to begin on the next block
///
/// Returns the AuctionData object created.
///
/// ### Arguments
/// * `auction_type` - The type of auction being created
///
/// ### Panics
/// If the auction is unable to be created
pub fn create(e: &Env, auction_type: u32) -> AuctionData {
    let backstop = storage::get_backstop(e);
    let auction_data = match AuctionType::from_u32(auction_type) {
        AuctionType::UserLiquidation => {
            panic_with_error!(e, PoolError::BadRequest);
        }
        AuctionType::BadDebtAuction => create_bad_debt_auction_data(e, &backstop),
        AuctionType::InterestAuction => create_interest_auction_data(e, &backstop),
    };

    storage::set_auction(e, &auction_type, &backstop, &auction_data);

    auction_data
}

/// Create a liquidation auction. Stores the resulting auction to the ledger to begin on the next block
///
/// Returns the AuctionData object created.
///
/// ### Arguments
/// * `user` - The user being liquidated
/// * `liq_data` - The liquidation metadata
///
/// ### Panics
/// If the auction is unable to be created
pub fn create_liquidation(e: &Env, user: &Address, percent_liquidated: u64) -> AuctionData {
    let auction_data = create_user_liq_auction_data(e, user, percent_liquidated);

    storage::set_auction(
        e,
        &(AuctionType::UserLiquidation as u32),
        user,
        &auction_data,
    );

    auction_data
}

/// Delete a liquidation auction if the user being liquidated
///
/// NOTE: Does not verify if the user's positions are healthy. This must be done before calling.
///
/// ### Arguments
/// * `auction_type` - The type of auction being created
///
/// ### Panics
/// If no auction exists for the user
pub fn delete_liquidation(e: &Env, user: &Address) {
    if !storage::has_auction(e, &(AuctionType::UserLiquidation as u32), user) {
        panic_with_error!(e, PoolError::BadRequest);
    }
    storage::del_auction(e, &(AuctionType::UserLiquidation as u32), user);
}

/// Fills the auction from the invoker. The filler is expected to maintain allowances to both
/// the pool and the backstop module.
///
/// TODO: Use auth-next to avoid required allowances
///
/// ### Arguments
/// * `pool` - The pool
/// * `auction_type` - The type of auction to fill
/// * `user` - The user involved in the auction
/// * `filler_state` - The Address filling the auction
/// * `percent_filled` - The percentage being filled as a number (i.e. 15 => 15%)
///
/// ### Panics
/// If the auction does not exist, or if the pool is unable to fulfill either side
/// of the auction quote
pub fn fill(
    e: &Env,
    pool: &mut Pool,
    auction_type: u32,
    user: &Address,
    filler_state: &mut User,
    percent_filled: u64,
) {
    let auction_data = storage::get_auction(e, &auction_type, user);
    if percent_filled > 100 || percent_filled == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let (to_fill_auction, remaining_auction) = scale_auction(e, &auction_data, percent_filled);
    match AuctionType::from_u32(auction_type) {
        AuctionType::UserLiquidation => {
            fill_user_liq_auction(e, pool, &to_fill_auction, user, filler_state)
        }
        AuctionType::BadDebtAuction => {
            fill_bad_debt_auction(e, pool, &to_fill_auction, filler_state)
        }
        AuctionType::InterestAuction => {
            fill_interest_auction(e, pool, &to_fill_auction, &filler_state.address)
        }
    };

    if let Some(auction_to_store) = remaining_auction {
        storage::set_auction(e, &auction_type, user, &auction_to_store);
    } else {
        storage::del_auction(e, &auction_type, user);
    }
}

/// Scale the auction based on the percent being filled and the amount of blocks that have passed
/// since the auction began.
///
/// ### Arguments
/// * `auction_data` - The auction data to scale
/// * `percent_filled` - The percentage being filled as a number (i.e. 15 => 15%)
///
/// Returns the (Scaled Auction, Remaining Auction) such that:
/// - Scaled Auction is the auction data scaled
/// - Remaining Auction is the leftover auction data that will be stored in the ledger, or deleted if None
#[allow(clippy::zero_prefixed_literal)]
fn scale_auction(
    e: &Env,
    auction_data: &AuctionData,
    percent_filled: u64,
) -> (AuctionData, Option<AuctionData>) {
    let mut to_fill_auction = AuctionData {
        bid: map![e],
        lot: map![e],
        block: auction_data.block,
    };
    let mut remaining_auction = AuctionData {
        bid: map![e],
        lot: map![e],
        block: auction_data.block,
    };

    // determine block based auction modifiers
    let bid_modifier: i128;
    let lot_modifier: i128;
    let per_block_scalar: i128 = 0_0050000; // modifier moves 0.5% every block
    let block_dif = i128(e.ledger().sequence() - auction_data.block);
    if block_dif > 200 {
        // lot 100%, bid scaling down from 100% to 0%
        lot_modifier = SCALAR_7;
        if block_dif < 400 {
            bid_modifier = SCALAR_7 - (block_dif - 200) * per_block_scalar;
        } else {
            bid_modifier = 0;
        }
    } else {
        // lot scaling from 0% to 100%, bid 100%
        lot_modifier = block_dif * per_block_scalar;
        bid_modifier = SCALAR_7;
    }

    // scale the auction
    let percent_filled_i128 = i128(percent_filled) * 1_00000; // scale to decimal form in 7 decimals from percentage
    for (asset, amount) in auction_data.bid.iter() {
        // apply percent scalar and store remainder to base auction
        // round up to avoid rounding exploits
        let to_fill_base = amount
            .fixed_mul_ceil(percent_filled_i128, SCALAR_7)
            .unwrap_optimized();
        let remaining_base = amount - to_fill_base;
        if remaining_base > 0 {
            remaining_auction.bid.set(asset.clone(), remaining_base);
        }
        // apply block scalar to to_fill auction and don't store if 0
        let to_fill_scaled = to_fill_base
            .fixed_mul_ceil(bid_modifier, SCALAR_7)
            .unwrap_optimized();
        if to_fill_scaled > 0 {
            to_fill_auction.bid.set(asset, to_fill_scaled);
        }
    }
    for (asset, amount) in auction_data.lot.iter() {
        // apply percent scalar and store remainder to base auction
        // round down to avoid rounding exploits
        let to_fill_base = amount
            .fixed_mul_floor(percent_filled_i128, SCALAR_7)
            .unwrap_optimized();
        let remaining_base = amount - to_fill_base;
        if remaining_base > 0 {
            remaining_auction.lot.set(asset.clone(), remaining_base);
        }
        // apply block scalar to to_fill auction and don't store if 0
        let to_fill_scaled = to_fill_base
            .fixed_mul_floor(lot_modifier, SCALAR_7)
            .unwrap_optimized();
        if to_fill_scaled > 0 {
            to_fill_auction.lot.set(asset, to_fill_scaled);
        }
    }

    if remaining_auction.lot.is_empty() && remaining_auction.bid.is_empty() {
        (to_fill_auction, None)
    } else {
        (to_fill_auction, Some(remaining_auction))
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        pool::Positions,
        storage::PoolConfig,
        testutils::{self, create_pool},
    };

    use super::*;
    use sep_40_oracle::testutils::Asset;
    use soroban_sdk::{
        map,
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, Symbol,
    };

    #[test]
    fn test_create_bad_debt_auction() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool_address = create_pool(&e);

        let (blnd, blnd_client) = testutils::create_blnd_token(&e, &pool_address, &bombadil);
        let (usdc, usdc_client) = testutils::create_usdc_token(&e, &pool_address, &bombadil);
        let (lp_token, lp_token_client) =
            testutils::create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &lp_token,
            &usdc,
            &blnd,
        );
        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_address, &50_000_0000000);
        backstop_client.update_tkn_val();

        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.d_rate = 1_100_000_000;
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.d_rate = 1_200_000_000;
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
            &Asset::Other(Symbol::new(&e, "USD1")),
            &vec![
                &e,
                Asset::Stellar(underlying_0),
                Asset::Stellar(underlying_1),
                Asset::Stellar(underlying_2),
                Asset::Stellar(usdc),
                Asset::Stellar(blnd),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![
            &e,
            2_0000000,
            4_0000000,
            100_0000000,
            1_0000000,
            0_1000000,
        ]);

        let positions: Positions = Positions {
            collateral: map![&e],
            liabilities: map![
                &e,
                (reserve_config_0.index, 10_0000000),
                (reserve_config_1.index, 2_5000000)
            ],
            supply: map![&e],
        };

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &backstop_address, &positions);

            create(&e, 1);
            assert!(storage::has_auction(&e, &1, &backstop_address));
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
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (usdc_id, _) = testutils::create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::generate(&e),
            &Address::generate(&e),
            &Address::generate(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
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
                Asset::Stellar(usdc_id),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 100_0000000, 1_0000000]);

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
            create(&e, 2);
            assert!(storage::has_auction(&e, &2, &backstop_address));
        });
    }

    #[test]
    fn test_create_liquidation() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (oracle_address, oracle_client) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.last_time = 12345;
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_config_0.c_factor = 0_8500000;
        reserve_config_0.l_factor = 0_9000000;
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
        reserve_data_1.b_rate = 1_200_000_000;
        reserve_config_1.c_factor = 0_7500000;
        reserve_config_1.l_factor = 0_7500000;
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
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
        reserve_config_2.c_factor = 0_0000000;
        reserve_config_2.l_factor = 0_7000000;
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
                Asset::Stellar(underlying_0),
                Asset::Stellar(underlying_1),
                Asset::Stellar(underlying_2),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 2_0000000, 4_0000000, 50_0000000]);

        let liq_pct = 45;
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            create_liquidation(&e, &samwise, liq_pct);
            assert!(storage::has_auction(&e, &0, &samwise));
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_create_user_liquidation_errors() {
        let e = Env::default();
        let pool_id = create_pool(&e);
        let backstop_id = Address::generate(&e);

        e.as_contract(&pool_id, || {
            storage::set_backstop(&e, &backstop_id);

            create(&e, AuctionType::UserLiquidation as u32);
        });
    }

    #[test]
    fn test_delete_user_liquidation() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_id = create_pool(&e);
        let samwise = Address::generate(&e);

        let auction_data = AuctionData {
            bid: map![&e],
            lot: map![&e],
            block: 100,
        };
        e.as_contract(&pool_id, || {
            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );

            delete_liquidation(&e, &samwise);
            assert!(!storage::has_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise
            ));
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_delete_user_liquidation_does_not_exist() {
        let e = Env::default();
        e.mock_all_auths();
        let pool_id = create_pool(&e);

        let samwise = Address::generate(&e);

        e.as_contract(&pool_id, || {
            delete_liquidation(&e, &samwise);
        });
    }

    #[test]
    fn test_fill() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta();
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta();
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 20,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 172800,
                min_persistent_entry_ttl: 172800,
                max_entry_ttl: 9999999,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 100);
            let has_auction = storage::has_auction(&e, &0, &samwise);
            assert_eq!(has_auction, false);
        });
    }

    #[test]
    fn test_partial_fill() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta();
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta();
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 20,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 172800,
                min_persistent_entry_ttl: 172800,
                max_entry_ttl: 9999999,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 25);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 9281250)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 22_9196497),
                    (underlying_1.clone(), 1_1546805)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);
        });
    }

    #[test]
    fn test_partial_partial_full_fill() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta();

        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta();

        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();

        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 100_000_0000)],
            lot: map![
                &e,
                (underlying_0.clone(), 10_000_0000),
                (underlying_1.clone(), 1_000_0000)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 30_000_0000),
                (reserve_config_1.index, 3_000_0000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 200_000_0000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            // Partial fill 1 - 25% @ 50% lot mod
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 100 * 5,
                protocol_version: 20,
                sequence_number: 176 + 100,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 172800,
                min_persistent_entry_ttl: 172800,
                max_entry_ttl: 9999999,
            });
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 25);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 75_000_0000)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 7_500_0000),
                    (underlying_1.clone(), 750_0000)
                ],
                block: 176,
            };

            // Partial fill 2 - 66% @ 100% mods
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 20,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 172800,
                min_persistent_entry_ttl: 172800,
                max_entry_ttl: 9999999,
            });
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 67);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 24_7500000)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 2_4750000),
                    (underlying_1.clone(), 0_2475000)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);

            // full fill at 50% bid mod
            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 300 * 5,
                protocol_version: 20,
                sequence_number: 176 + 300,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 172800,
                min_persistent_entry_ttl: 172800,
                max_entry_ttl: 9999999,
            });
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 100);
            let new_auction = storage::has_auction(&e, &0, &samwise);
            assert_eq!(new_auction, false);
            let samwise_positions = storage::get_user_positions(&e, &samwise);
            assert_eq!(
                samwise_positions
                    .collateral
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                30_000_0000 - 1_250_0000 - 5_000_0002 - 2_499_9998
            );
            assert_eq!(
                samwise_positions
                    .collateral
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                3_000_0000 - 125_0000 - 500_0000 - 250_0000
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_2.index)
                    .unwrap_optimized(),
                200_000_0000 - 25_000_0000 - 50_000_0025 - 12_6249975
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_fill_fails_pct_too_large() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta();
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta();
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 20,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 172800,
                min_persistent_entry_ttl: 172800,
                max_entry_ttl: 9999999,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 101);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 9281250)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 22_9196497),
                    (underlying_1.clone(), 1_1546805)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_fill_fails_pct_too_small() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 175,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, reserve_data_0) = testutils::default_reserve_meta();
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, reserve_data_1) = testutils::default_reserve_meta();

        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();

        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        e.budget().reset_unlimited();
        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(&e, &0, &samwise, &auction_data);

            e.ledger().set(LedgerInfo {
                timestamp: 12345 + 200 * 5,
                protocol_version: 20,
                sequence_number: 176 + 200,
                network_id: Default::default(),
                base_reserve: 10,
                min_temp_entry_ttl: 172800,
                min_persistent_entry_ttl: 172800,
                max_entry_ttl: 9999999,
            });
            e.budget().reset_unlimited();
            let mut pool = Pool::load(&e);
            let mut frodo_state = User::load(&e, &frodo);
            fill(&e, &mut pool, 0, &samwise, &mut frodo_state, 0);

            let expected_new_auction_data = AuctionData {
                bid: map![&e, (underlying_2.clone(), 9281250)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 22_9196497),
                    (underlying_1.clone(), 1_1546805)
                ],
                block: 176,
            };
            let new_auction = storage::get_auction(&e, &0, &samwise);
            assert_eq!(new_auction.bid, expected_new_auction_data.bid);
            assert_eq!(new_auction.lot, expected_new_auction_data.lot);
            assert_eq!(new_auction.block, expected_new_auction_data.block);
        });
    }

    #[test]
    fn test_scale_auction_100_fill_pct() {
        // 0 blocks
        let e = Env::default();
        let underlying_0 = Address::generate(&e);
        let underlying_1 = Address::generate(&e);

        let base_auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 100_0000000)],
            lot: map![&e, (underlying_1.clone(), 100_0000000)],
            block: 1000,
        };

        // 0 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });
        let (scaled_auction, remaining_auction) = scale_auction(&e, &base_auction_data, 100);
        assert_eq!(
            scaled_auction.bid.get_unchecked(underlying_0.clone()),
            100_0000000
        );
        assert_eq!(scaled_auction.lot.len(), 0);
        assert!(remaining_auction.is_none());

        // 100 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });
        let (scaled_auction, remaining_auction) = scale_auction(&e, &base_auction_data, 100);
        assert_eq!(
            scaled_auction.bid.get_unchecked(underlying_0.clone()),
            100_0000000
        );
        assert_eq!(
            scaled_auction.lot.get_unchecked(underlying_1.clone()),
            50_0000000
        );
        assert!(remaining_auction.is_none());

        // 200 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1200,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });
        let (scaled_auction, remaining_auction) = scale_auction(&e, &base_auction_data, 100);
        assert_eq!(
            scaled_auction.bid.get_unchecked(underlying_0.clone()),
            100_0000000
        );
        assert_eq!(
            scaled_auction.lot.get_unchecked(underlying_1.clone()),
            100_0000000
        );
        assert!(remaining_auction.is_none());

        // 300 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1300,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });
        let (scaled_auction, remaining_auction) = scale_auction(&e, &base_auction_data, 100);
        assert_eq!(
            scaled_auction.bid.get_unchecked(underlying_0.clone()),
            50_0000000
        );
        assert_eq!(
            scaled_auction.lot.get_unchecked(underlying_1.clone()),
            100_0000000
        );
        assert!(remaining_auction.is_none());

        // 400 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1400,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });
        let (scaled_auction, remaining_auction) = scale_auction(&e, &base_auction_data, 100);
        assert_eq!(scaled_auction.bid.len(), 0);
        assert_eq!(
            scaled_auction.lot.get_unchecked(underlying_1.clone()),
            100_0000000
        );
        assert!(remaining_auction.is_none());
    }

    #[test]
    fn test_scale_auction_not_100_fill_pct() {
        // @dev: bids always round up, lots always round down
        //       the remaining is exact based on scaled auction
        let e = Env::default();
        let underlying_0 = Address::generate(&e);
        let underlying_1 = Address::generate(&e);

        let base_auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 25_0000005)],
            lot: map![&e, (underlying_1.clone(), 25_0000005)],
            block: 1000,
        };

        // 0 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });
        let (scaled_auction, remaining_auction_option) = scale_auction(&e, &base_auction_data, 50);
        let remaining_auction = remaining_auction_option.unwrap();
        assert_eq!(
            scaled_auction.bid.get_unchecked(underlying_0.clone()),
            12_5000003 // fill pct rounds up
        );
        assert_eq!(scaled_auction.lot.len(), 0);
        assert_eq!(
            remaining_auction.bid.get_unchecked(underlying_0.clone()),
            12_5000002
        );
        assert_eq!(
            remaining_auction.lot.get_unchecked(underlying_1.clone()),
            12_5000003
        );

        // 100 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });

        let (scaled_auction, remaining_auction_option) = scale_auction(&e, &base_auction_data, 60);
        let remaining_auction = remaining_auction_option.unwrap();
        assert_eq!(
            scaled_auction.bid.get_unchecked(underlying_0.clone()),
            15_0000003
        );
        assert_eq!(
            scaled_auction.lot.get_unchecked(underlying_1.clone()),
            7_5000001 // modifier rounds down
        );
        assert_eq!(
            remaining_auction.bid.get_unchecked(underlying_0.clone()),
            10_0000002
        );
        assert_eq!(
            remaining_auction.lot.get_unchecked(underlying_1.clone()),
            10_0000002
        );

        // 300 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1300,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });

        let (scaled_auction, remaining_auction_option) = scale_auction(&e, &base_auction_data, 60);
        let remaining_auction = remaining_auction_option.unwrap();
        assert_eq!(
            scaled_auction.bid.get_unchecked(underlying_0.clone()),
            7_5000002 // modifier rounds up
        );
        assert_eq!(
            scaled_auction.lot.get_unchecked(underlying_1.clone()),
            15_0000003
        );
        assert_eq!(
            remaining_auction.bid.get_unchecked(underlying_0.clone()),
            10_0000002
        );
        assert_eq!(
            remaining_auction.lot.get_unchecked(underlying_1.clone()),
            10_0000002
        );

        // 400 blocks
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 1400,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 172800,
            min_persistent_entry_ttl: 172800,
            max_entry_ttl: 9999999,
        });
        let (scaled_auction, remaining_auction_option) = scale_auction(&e, &base_auction_data, 50);
        let remaining_auction = remaining_auction_option.unwrap();
        assert_eq!(scaled_auction.bid.len(), 0);
        assert_eq!(
            scaled_auction.lot.get_unchecked(underlying_1.clone()),
            12_5000002 // fill pct rounds down
        );
        assert_eq!(
            remaining_auction.bid.get_unchecked(underlying_0.clone()),
            12_5000002
        );
        assert_eq!(
            remaining_auction.lot.get_unchecked(underlying_1.clone()),
            12_5000003
        );
    }
}
