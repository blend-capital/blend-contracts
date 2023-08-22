use crate::{
    constants::SCALAR_7,
    dependencies::BackstopClient,
    errors::PoolError,
    pool::{burn_backstop_bad_debt, Pool, User},
    storage,
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, panic_with_error, unwrap::UnwrapOptimized, Address, Env};

use super::{AuctionData, AuctionType};

pub fn create_bad_debt_auction_data(e: &Env, backstop: &Address) -> AuctionData {
    if storage::has_auction(e, &(AuctionType::BadDebtAuction as u32), backstop) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }

    let mut auction_data = AuctionData {
        bid: map![e],
        lot: map![e],
        block: e.ledger().sequence() + 1,
    };

    let mut pool = Pool::load(e);
    let backstop_positions = storage::get_user_positions(e, backstop);
    let reserve_list = storage::get_res_list(e);
    let mut debt_value = 0;
    for (reserve_index, liability_balance) in backstop_positions.liabilities.iter() {
        let res_asset_address = reserve_list.get_unchecked(reserve_index);
        if liability_balance > 0 {
            let reserve = pool.load_reserve(e, &res_asset_address);
            let asset_to_base = pool.load_price(e, &res_asset_address);
            let asset_balance = reserve.to_asset_from_d_token(liability_balance);
            debt_value += i128(asset_to_base)
                .fixed_mul_floor(asset_balance, reserve.scalar)
                .unwrap_optimized();
            auction_data.bid.set(res_asset_address, liability_balance);
        }
    }
    if auction_data.bid.is_empty()  || debt_value == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let backstop_client = BackstopClient::new(e, backstop);
    let backstop_token = backstop_client.backstop_token();
    // TODO: This won't have an oracle entry. Once an LP implementation exists, unwrap base from LP
    let backstop_token_to_base = pool.load_price(e, &backstop_token);
    let mut lot_amount = debt_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap_optimized()
        .fixed_div_floor(i128(backstop_token_to_base), SCALAR_7)
        .unwrap_optimized();
    let pool_balance = backstop_client.pool_balance(&e.current_contract_address());
    lot_amount = pool_balance.tokens.min(lot_amount);
    // u32::MAX is the key for the backstop token
    auction_data.lot.set(backstop_token, lot_amount);

    auction_data
}

#[allow(clippy::inconsistent_digit_grouping)]
pub fn fill_bad_debt_auction(
    e: &Env,
    pool: &mut Pool,
    auction_data: &AuctionData,
    filler_state: &mut User,
) {
    let backstop_address = storage::get_backstop(e);
    let mut backstop_state = User::load(e, &backstop_address);

    // bid only contains d_token asset amounts
    backstop_state.rm_positions(e, pool, map![e], auction_data.bid.clone());
    filler_state.add_positions(e, pool, map![e], auction_data.bid.clone());

    let backstop_client = BackstopClient::new(e, &backstop_address);
    let backstop_token_id = backstop_client.backstop_token();
    let lot_amount = auction_data.lot.get(backstop_token_id).unwrap_optimized();
    let backstop_client = BackstopClient::new(e, &backstop_address);
    backstop_client.draw(
        &e.current_contract_address(),
        &lot_amount,
        &filler_state.address,
    );

    // If the backstop still has liabilities and less than 10% of the backstop threshold burn bad debt
    if !backstop_state.positions.liabilities.is_empty() 
            //TODO: this token check needs to check k-value of pool balance LP tokens
        && backstop_client.pool_balance(&e.current_contract_address()).tokens < 20_000_000_0000
    {
        burn_backstop_bad_debt(e, &mut backstop_state, pool)
    }
    backstop_state.store(e);
}

#[cfg(test)]
mod tests {

    use crate::{auctions::auction::AuctionType, pool::Positions, storage::PoolConfig, testutils};

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        unwrap::UnwrapOptimized,
    };

    #[test]
    #[should_panic]
    //#[should_panic(expected = "ContractError(103)")]
    fn test_create_bad_debt_auction_already_in_progress() {
        let e = Env::default();
        e.budget().reset_unlimited(); // setup exhausts budget

        let pool_address = Address::random(&e);
        let (backstop_address, _backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );

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
                &(AuctionType::BadDebtAuction as u32),
                &backstop_address,
                &auction_data,
            );

            create_bad_debt_auction_data(&e, &backstop_address);
        });
    }

    #[test]
    fn test_create_bad_debt_auction() {
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
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
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

        backstop_token_client.mint(&samwise, &200_0000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &100_0000000);

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&backstop_token_id, &0_5000000);

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

            let result = create_bad_debt_auction_data(&e, &backstop_address);

            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(underlying_0), 10_0000000);
            assert_eq!(result.bid.get_unchecked(underlying_1), 2_5000000);
            assert_eq!(result.bid.len(), 2);
            assert_eq!(result.lot.get_unchecked(backstop_token_id), 95_2000000);
            assert_eq!(result.lot.len(), 1);
        });
    }

    #[test]
    fn test_create_bad_debt_auction_max_balance() {
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
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
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

        backstop_token_client.mint(&samwise, &200_0000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &95_0000000);

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&backstop_token_id, &0_5000000);

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

            let result = create_bad_debt_auction_data(&e, &backstop_address);

            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(underlying_0), 10_0000000);
            assert_eq!(result.bid.get_unchecked(underlying_1), 2_5000000);
            assert_eq!(result.bid.len(), 2);
            assert_eq!(result.lot.get_unchecked(backstop_token_id), 95_0000000);
            assert_eq!(result.lot.len(), 1);
        });
    }

    #[test]
    fn test_create_bad_debt_auction_applies_interest() {
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
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::random(&e),
        );

        let (oracle_id, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
        reserve_data_0.d_rate = 1_100_000_000;
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
        reserve_data_1.d_rate = 1_200_000_000;
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
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        backstop_token_client.mint(&samwise, &200_0000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &100_0000000);

        oracle_client.set_price(&underlying_0, &2_0000000);
        oracle_client.set_price(&underlying_1, &4_0000000);
        oracle_client.set_price(&underlying_2, &100_0000000);
        oracle_client.set_price(&backstop_token_id, &0_5000000);

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
            storage::set_backstop(&e, &backstop_address);
            storage::set_user_positions(&e, &backstop_address, &positions);

            let result = create_bad_debt_auction_data(&e, &backstop_address);

            assert_eq!(result.block, 151);
            assert_eq!(result.bid.get_unchecked(underlying_0), 10_0000000);
            assert_eq!(result.bid.get_unchecked(underlying_1), 2_5000000);
            assert_eq!(result.bid.len(), 2);
            assert_eq!(result.lot.get_unchecked(backstop_token_id), 95_2004736);
            assert_eq!(result.lot.len(), 1);
        });
    }
    #[test]
    fn test_fill_bad_debt_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 51,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::random(&e),
        );

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
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
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let mut auction_data = AuctionData {
            bid: map![&e, (underlying_0, 10_0000000), (underlying_1, 2_5000000)],
            lot: map![&e, (backstop_token_id.clone(), 47_6000000)],
            block: 51,
        };
        let positions: Positions = Positions {
            collateral: map![&e],
            liabilities: map![
                &e,
                (reserve_config_0.index, 10_0000000),
                (reserve_config_1.index, 2_5000000)
            ],
            supply: map![&e],
        };
        backstop_token_client.mint(&samwise, &95_2000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &95_2000000);
        e.as_contract(&pool_address, || {
            storage::set_auction(
                &e,
                &(AuctionType::BadDebtAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &backstop_address, &positions);

            backstop_token_client.approve(
                &pool_address,
                &backstop_address,
                &(u64::MAX as i128),
                &1000000,
            );
            let mut pool = Pool::load(&e);
            let mut samwise_state = User::load(&e, &samwise);
            fill_bad_debt_auction(&e, &mut pool, &mut auction_data, &mut samwise_state);
            assert_eq!(backstop_token_client.balance(&backstop_address), 47_6000000);
            assert_eq!(backstop_token_client.balance(&samwise), 47_6000000);
            let samwise_positions = samwise_state.positions;
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                10_0000000
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                2_5000000
            );
            let backstop_positions = storage::get_user_positions(&e, &backstop_address);
            assert_eq!(backstop_positions.liabilities.len(), 0);
        });
    }

    #[test]
    fn test_fill_bad_debt_auction_leftover_debt() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 51,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::random(&e),
        );

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
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
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let mut auction_data = AuctionData {
            bid: map![
                &e,
                (underlying_0, 10_0000000 - 2_5000000),
                (underlying_1, 2_5000000 - 6250000)
            ],
            lot: map![&e, (backstop_token_id.clone(), 47_6000000)],
            block: 51,
        };
        let positions: Positions = Positions {
            collateral: map![&e],
            liabilities: map![
                &e,
                (reserve_config_0.index, 10_0000000),
                (reserve_config_1.index, 2_5000000)
            ],
            supply: map![&e],
        };
        backstop_token_client.mint(&samwise, &95_2000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &95_2000000);
        e.as_contract(&pool_address, || {
            storage::set_auction(
                &e,
                &(AuctionType::BadDebtAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &backstop_address, &positions);

            backstop_token_client.approve(
                &pool_address,
                &backstop_address,
                &(u64::MAX as i128),
                &1000000,
            );
            let mut pool = Pool::load(&e);
            let mut samwise_state = User::load(&e, &samwise);
            fill_bad_debt_auction(&e, &mut pool, &mut auction_data, &mut samwise_state);
            assert_eq!(backstop_token_client.balance(&backstop_address), 47_6000000);
            assert_eq!(backstop_token_client.balance(&samwise), 47_6000000);
            let samwise_positions = samwise_state.positions;
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                10_0000000 - 2_5000000
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                2_5000000 - 6250000
            );
            let backstop_positions = storage::get_user_positions(&e, &backstop_address);
            assert_eq!(backstop_positions.liabilities.len(), 0);
        });
    }

    #[test]
    fn test_fill_bad_debt_auction_leftover_debt_sufficient_balance() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 51,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::random(&e),
        );

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
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
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let mut auction_data = AuctionData {
            bid: map![
                &e,
                (underlying_0, 10_0000000 - 2_5000000),
                (underlying_1, 2_5000000 - 6250000)
            ],
            lot: map![&e, (backstop_token_id.clone(), 47_6000000)],
            block: 51,
        };
        let positions: Positions = Positions {
            collateral: map![&e],
            liabilities: map![
                &e,
                (reserve_config_0.index, 10_0000000),
                (reserve_config_1.index, 2_5000000)
            ],
            supply: map![&e],
        };
        backstop_token_client.mint(&samwise, &100_095_2000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &100_095_2000000);
        e.as_contract(&pool_address, || {
            storage::set_auction(
                &e,
                &(AuctionType::BadDebtAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &backstop_address, &positions);

            backstop_token_client.approve(
                &pool_address,
                &backstop_address,
                &(u64::MAX as i128),
                &1000000,
            );
            let mut pool = Pool::load(&e);
            let mut samwise_state = User::load(&e, &samwise);
            fill_bad_debt_auction(&e, &mut pool, &mut auction_data, &mut samwise_state);
            assert_eq!(
                backstop_token_client.balance(&backstop_address),
                100_047_6000000
            );
            assert_eq!(backstop_token_client.balance(&samwise), 47_6000000);
            let samwise_positions = samwise_state.positions;
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                10_0000000 - 2_5000000
            );
            assert_eq!(
                samwise_positions
                    .liabilities
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                2_5000000 - 6250000
            );
            let backstop_positions = storage::get_user_positions(&e, &backstop_address);
            assert_eq!(
                backstop_positions
                    .liabilities
                    .get(reserve_config_0.index)
                    .unwrap_optimized(),
                2_5000000
            );
            assert_eq!(
                backstop_positions
                    .liabilities
                    .get(reserve_config_1.index)
                    .unwrap_optimized(),
                6250000
            );
        });
    }
}
