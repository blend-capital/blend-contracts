use soroban_sdk::{map, panic_with_error, Address, Env, Symbol};

use crate::{
    errors::PoolError,
    storage::{self},
};

use super::{user::User, Pool};

/// Transfer bad debt from a user to the backstop. Validates that the user does hold bad debt
/// and transfers all held d_tokens to the backstop.
///
/// ### Arguments
/// * `user` - The user who has bad debt
///
/// ### Panics
/// If the user does not have bad debt
pub fn transfer_bad_debt_to_backstop(e: &Env, user: &Address) {
    let backstop_address = storage::get_backstop(e);
    if user.clone() == backstop_address {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let user_state = User::load(e, user);
    if !user_state.positions.collateral.is_empty() || user_state.positions.liabilities.is_empty() {
        panic_with_error!(e, PoolError::BadRequest);
    }

    // the user does not have collateral and currently holds a liability meaning they hold bad debt
    // transfer all of the user's debt to the backstop
    let mut pool = Pool::load(e);
    let reserve_list = storage::get_res_list(e);
    let backstop_state = User::load(e, &backstop_address);
    let mut new_user_state = user_state.clone();
    let mut new_backstop_state = backstop_state.clone();
    for (reserve_index, liability_balance) in user_state.positions.liabilities.iter() {
        let asset = reserve_list.get_unchecked(reserve_index);
        let mut reserve = pool.load_reserve(e, &asset);
        new_backstop_state.add_liabilities(e, &mut reserve, liability_balance);
        new_user_state.remove_liabilities(e, &mut reserve, liability_balance);
        pool.cache_reserve(reserve, true);

        e.events().publish(
            (Symbol::new(e, "bad_debt"), user),
            (asset, liability_balance),
        );
    }

    pool.store_cached_reserves(e);
    new_backstop_state.store(e);
    new_user_state.store(e);
}

/// Burn bad debt from the backstop. This can only occur if the backstop module has reached a critical balance
pub fn burn_backstop_bad_debt(e: &Env, backstop: &mut User, pool: &mut Pool) {
    let reserve_list = storage::get_res_list(e);
    let mut rm_liabilities = map![e];
    for (reserve_index, liability_balance) in backstop.positions.liabilities.iter() {
        let res_asset_address = reserve_list.get_unchecked(reserve_index);
        rm_liabilities.set(res_asset_address.clone(), liability_balance);

        e.events().publish(
            (Symbol::new(e, "bad_debt"), backstop.address.clone()),
            (res_asset_address, liability_balance),
        );
    }
    // remove liability debtTokens from backstop resulting in a shared loss for
    // token suppliers
    backstop.rm_positions(e, pool, map![e], rm_liabilities);
}

#[cfg(test)]
mod tests {
    use crate::{pool::Positions, storage::PoolConfig, testutils};

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
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let backstop = Address::generate(&e);

        let samwise = Address::generate(&e);
        let bombadil = Address::generate(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, reserve_data) = testutils::default_reserve_meta();
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
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
            transfer_bad_debt_to_backstop(&e, &samwise);

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
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_transfer_bad_debt_with_collateral_panics() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let backstop = Address::generate(&e);

        let samwise = Address::generate(&e);
        let bombadil = Address::generate(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, reserve_data) = testutils::default_reserve_meta();
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
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

            transfer_bad_debt_to_backstop(&e, &samwise);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_transfer_bad_debt_without_liabilities_panics() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let backstop = Address::generate(&e);

        let samwise = Address::generate(&e);
        let bombadil = Address::generate(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, reserve_data) = testutils::default_reserve_meta();
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let user_positions = Positions::env_default(&e);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &samwise, &user_positions);

            e.budget().reset_unlimited();
            transfer_bad_debt_to_backstop(&e, &samwise);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_transfer_bad_debt_with_backstop_panics() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let backstop = Address::generate(&e);

        let samwise = Address::generate(&e);
        let bombadil = Address::generate(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, reserve_data) = testutils::default_reserve_meta();
        reserve_config.index = 1;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
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
            transfer_bad_debt_to_backstop(&e, &backstop);
        });
    }

    #[test]
    fn test_burn_backstop_bad_debt() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let backstop = Address::generate(&e);

        let (_, blnd_client) = testutils::create_blnd_token(&e, &pool, &bombadil);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.last_time = 1499995000;
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_config.index = 1;
        reserve_data.last_time = 1499995000;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        blnd_client.mint(&backstop, &123);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
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

            let mut pool_obj = Pool::load(&e);
            let mut backstop_user = User::load(&e, &backstop);
            e.budget().reset_unlimited();
            burn_backstop_bad_debt(&e, &mut backstop_user, &mut pool_obj);

            assert_eq!(backstop_user.positions.collateral.len(), 0);
            assert_eq!(backstop_user.positions.liabilities.len(), 0);
        });
    }
}
