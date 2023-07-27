use soroban_sdk::{map, panic_with_error, Address, Env, Symbol};

use crate::{
    dependencies::TokenClient,
    errors::PoolError,
    storage::{self, has_auction},
};

use super::{user::User, Pool, Reserve};

/// Manage bad debt for a user. If the user is the backstop, burn the bad debt. Otherwise, transfer
/// the bad debt to the backstop.
///
/// ### Panics
/// If the user does not have bad debt or the backstop's bad debt is not eligible to be burnt
pub fn manage_bad_debt(e: &Env, user: &Address) {
    let backstop = storage::get_backstop(e);
    if user.clone() == backstop {
        burn_backstop_bad_debt(e, &backstop);
    } else {
        transfer_bad_debt_to_backstop(e, user, &backstop);
    }
}

/// Transfer bad debt from a user to the backstop. Validates that the user does hold bad debt
/// and transfers all held d_tokens to the backstop.
///
/// ### Arguments
/// * `user` - The user who has bad debt
///
/// ### Panics
/// If the user does not have bad debt
fn transfer_bad_debt_to_backstop(e: &Env, user: &Address, backstop: &Address) {
    let user_state = User::load(e, user);
    if user_state.positions.collateral.len() != 0 || user_state.positions.liabilities.len() == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    // the user does not have collateral and currently holds a liability meaning they hold bad debt
    // transfer all of the user's debt to the backstop
    let mut pool = Pool::load(e);
    let reserve_list = storage::get_res_list(e);
    let backstop_state = User::load(e, backstop);
    let mut new_user_state = user_state.clone();
    let mut new_backstop_state = backstop_state.clone();
    for (reserve_index, liability_balance) in user_state.positions.liabilities.iter() {
        let asset = reserve_list.get_unchecked(reserve_index);
        let mut reserve = pool.load_reserve(e, &asset);
        new_backstop_state.add_liabilities(e, &mut reserve, liability_balance);
        new_user_state.remove_liabilities(e, &mut reserve, liability_balance);
        pool.cache_reserve(reserve, true);

        e.events().publish(
            (Symbol::new(&e, "bad_debt"), user),
            (asset, liability_balance),
        );
    }

    pool.store_cached_reserves(e);
    new_backstop_state.store(e);
    new_user_state.store(e);
}

/// Burn bad debt from the backstop. This can only occur if the backstop module has reached a critical balance and
/// a bad debt auction is not currently occurring.
///
/// ### Panics
/// If the backstop's bad debt is not eligible to be burnt
fn burn_backstop_bad_debt(e: &Env, backstop: &Address) {
    if has_auction(e, &1, backstop) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }

    // TODO: Determine a healthy number for this
    let backstop_balance = TokenClient::new(e, &storage::get_blnd_token(e)).balance(backstop);
    if backstop_balance > 10_000_0000000 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let mut backstop_positions = storage::get_user_positions(e, backstop);

    let pool_config = storage::get_pool_config(e);
    let reserve_list = storage::get_res_list(e);
    for (reserve_index, liability_balance) in backstop_positions.liabilities.iter() {
        if liability_balance > 0 {
            // remove liability debtTokens from backstop resulting in a shared loss for
            // token suppliers
            let res_asset_address = reserve_list.get_unchecked(reserve_index);
            let mut reserve = Reserve::load(e, &pool_config, &res_asset_address);
            reserve.d_supply -= liability_balance;
            reserve.store(e);

            e.events().publish(
                (Symbol::new(&e, "bad_debt"), backstop),
                (res_asset_address, liability_balance),
            );
        }
    }
    backstop_positions.liabilities = map![e];

    storage::set_user_positions(e, &backstop, &backstop_positions);
}

#[cfg(test)]
mod tests {
    use crate::{auctions::AuctionData, pool::Positions, storage::PoolConfig, testutils};

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, Ledger, LedgerInfo},
    };

    /***** transfer_bad_debt_to_backstop ******/

    #[test]
    fn test_transfer_bad_debt_happy_path() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let backstop = Address::random(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let user_positions = Positions {
            liabilities: map![&e, (0, 24_0000000), (1, 25_0000000)],
            collateral: map![&e],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &samwise, &user_positions);

            e.budget().reset_unlimited();
            manage_bad_debt(&e, &samwise);

            let new_user_positions = storage::get_user_positions(&e, &samwise);
            let new_backstop_positions = storage::get_user_positions(&e, &backstop);
            assert_eq!(new_user_positions.collateral.len(), 0);
            assert_eq!(new_user_positions.liabilities.len(), 0);
            assert_eq!(
                new_backstop_positions.liabilities.get_unchecked(0),
                24_0000000
            );
            assert_eq!(
                new_backstop_positions.liabilities.get_unchecked(1),
                25_0000000
            );
        });
    }

    #[test]
    #[should_panic]
    // #[should_panic(expected = "Status(ContractError(2))")]
    fn test_transfer_bad_debt_with_collateral_panics() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let backstop = Address::random(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let user_positions = Positions {
            liabilities: map![&e, (0, 24_0000000), (1, 25_0000000)],
            collateral: map![&e, (0, 1)],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &samwise, &user_positions);

            manage_bad_debt(&e, &samwise);
        });
    }

    #[test]
    #[should_panic]
    // #[should_panic(expected = "Status(ContractError(2))")]
    fn test_transfer_bad_debt_without_liabilities_panics() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let backstop = Address::random(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let user_positions = Positions::env_default(&e);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &samwise, &user_positions);

            e.budget().reset_unlimited();
            manage_bad_debt(&e, &samwise);
        });
    }

    /***** burn_backstop_bad_debt ******/

    #[test]
    fn test_burn_backstop_bad_debt() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);

        let pool = Address::random(&e);
        let backstop = Address::random(&e);

        let (_, blnd_client) = testutils::create_blnd_token(&e, &pool, &bombadil);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499995000;
        let initial_d_supply_0 = reserve_data.d_supply;
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 1;
        reserve_data.last_time = 1499995000;
        let initial_d_supply_1 = reserve_data.d_supply;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        blnd_client.mint(&backstop, &123);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        let backstop_positions = Positions {
            liabilities: map![&e, (0, 24_0000000), (1, 25_0000000)],
            collateral: map![&e],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &backstop, &backstop_positions);

            e.budget().reset_unlimited();
            manage_bad_debt(&e, &backstop);

            let new_backstop_positions = storage::get_user_positions(&e, &backstop);
            assert_eq!(new_backstop_positions.collateral.len(), 0);
            assert_eq!(new_backstop_positions.liabilities.len(), 0);

            let reserve_1_data = storage::get_res_data(&e, &underlying_0);
            let reserve_2_data = storage::get_res_data(&e, &underlying_1);
            assert_eq!(reserve_1_data.last_time, 1500000000);
            assert_eq!(reserve_1_data.d_supply, initial_d_supply_0 - 24_0000000);
            assert_eq!(reserve_2_data.last_time, 1500000000);
            assert_eq!(reserve_2_data.d_supply, initial_d_supply_1 - 25_0000000);
        });
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(2))")]
    fn test_burn_backstop_bad_debt_with_balance_panics() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);

        let pool = Address::random(&e);
        let backstop = Address::random(&e);

        let (_, blnd_client) = testutils::create_blnd_token(&e, &pool, &bombadil);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499995000;
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499995000;
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        blnd_client.mint(&backstop, &10_000_0000001);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        let backstop_positions = Positions {
            liabilities: map![&e, (0, 24_0000000), (1, 25_0000000)],
            collateral: map![&e],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &backstop, &backstop_positions);

            manage_bad_debt(&e, &backstop);
        });
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(103))")]
    fn test_burn_backstop_bad_debt_with_auction_panics() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);

        let pool = Address::random(&e);
        let backstop = Address::random(&e);

        let (_, blnd_client) = testutils::create_blnd_token(&e, &pool, &bombadil);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499995000;
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499995000;
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        blnd_client.mint(&backstop, &10_0000001);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        let backstop_positions = Positions {
            liabilities: map![&e, (0, 24_0000000), (1, 25_0000000)],
            collateral: map![&e],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &backstop, &backstop_positions);

            // mock a created liquidation auction
            storage::set_auction(
                &e,
                &1,
                &backstop,
                &AuctionData {
                    bid: map![&e],
                    lot: map![&e],
                    block: e.ledger().sequence(),
                },
            );

            manage_bad_debt(&e, &backstop);
        });
    }
}
