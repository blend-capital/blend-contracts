use soroban_sdk::Map;
use soroban_sdk::{
    contracttype, panic_with_error, unwrap::UnwrapOptimized, Address, Env, Symbol, Vec,
};

use crate::{
    auctions, emissions, errors::PoolError, pool::Positions, storage,
    validator::require_nonnegative,
};

use super::{pool::Pool, Reserve};

/// An request a user makes against the pool
#[derive(Clone)]
#[contracttype]
pub struct Request {
    pub request_type: u32,
    pub address: Address, // asset address or liquidatee
    pub amount: i128,
}

/// Build a set of pool actions and the new positions from the supplied requests. Validates that the requests
/// are valid based on the status and supported reserves in the pool.
///
/// ### Arguments
/// * pool - The pool
/// * from - The sender of the requests
/// * requests - The requests to be processed
///
/// ### Returns
/// A tuple of (actions, positions, check_health) where:
/// * actions - A vec of actions to be taken by the pool
/// * positions - The final positions after the actions are taken
/// * check_health - A bool indicating if a health factor check should be performed
///
/// ### Panics
/// If the request is invalid, or if the pool is in an invalid state.
// @dev: Emissions update calls are performed on each request before b or d supply has a chance to change. If the same
//       reserve token is included twice in a request, the update emissions function will short circuit and not update
//       the emissions, preventing any inaccuracies.
pub fn build_actions_from_request(
    e: &Env,
    pool: &mut Pool,
    from: &Address,
    requests: Vec<Request>,
) -> (Map<Address, i128>, Positions, bool) {
    let mut actions: Map<Address, i128> = Map::new(&e); //tokens in is positive, tokens out is negative
    let old_positions = storage::get_user_positions(e, from);
    let mut new_positions = old_positions.clone();
    let mut check_health = false;
    for request in requests.iter() {
        // verify reserve is supported in the pool and the action is allowed
        require_nonnegative(e, &request.amount);
        pool.require_action_allowed(e, request.request_type);
        let mut reserve: Reserve;
        match request.request_type {
            0 => {
                // TODO: do we need to add error logic here for invalid reserve address?
                reserve = pool.load_reserve(e, &request.address);
                // supply
                emissions::update_emissions(
                    e,
                    reserve.index * 2 + 1,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(reserve.index),
                    false,
                );
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                reserve.b_supply += b_tokens_minted;
                new_positions.add_supply(e, &reserve, b_tokens_minted);
                actions.set(
                    reserve.asset.clone(),
                    actions
                        .get(request.address.clone())
                        .or(Some(0))
                        .unwrap_optimized()
                        + request.amount,
                );
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "supply"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, b_tokens_minted),
                );
            }
            1 => {
                // TODO: do we need to add error logic here for invalid reserve address?
                reserve = pool.load_reserve(e, &request.address);
                // withdraw
                emissions::update_emissions(
                    e,
                    reserve.index * 2 + 1,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(reserve.index),
                    false,
                );
                let cur_b_tokens = new_positions.get_supply(reserve.index);
                let mut to_burn = reserve.to_b_token_up(request.amount);
                let mut tokens_out = request.amount;
                if to_burn > cur_b_tokens {
                    to_burn = cur_b_tokens;
                    tokens_out = reserve.to_asset_from_b_token(cur_b_tokens);
                }
                reserve.b_supply -= to_burn;
                new_positions.remove_supply(e, &reserve, to_burn);
                actions.set(
                    reserve.asset.clone(),
                    actions
                        .get(request.address.clone())
                        .or(Some(0))
                        .unwrap_optimized()
                        - tokens_out,
                );
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "withdraw"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (tokens_out, to_burn),
                );
            }
            2 => {
                // TODO: do we need to add error logic here for invalid reserve address?
                reserve = pool.load_reserve(e, &request.address);
                // supply collateral
                emissions::update_emissions(
                    e,
                    reserve.index * 2 + 1,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(reserve.index),
                    false,
                );
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                reserve.b_supply += b_tokens_minted;
                new_positions.add_collateral(e, &reserve, b_tokens_minted);
                actions.set(
                    reserve.asset.clone(),
                    actions
                        .get(request.address.clone())
                        .or(Some(0))
                        .unwrap_optimized()
                        + request.amount,
                );
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "supply_collateral"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, b_tokens_minted),
                );
            }
            3 => {
                // TODO: do we need to add error logic here for invalid reserve address?
                reserve = pool.load_reserve(e, &request.address);
                // withdraw collateral
                emissions::update_emissions(
                    e,
                    reserve.index * 2 + 1,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(reserve.index),
                    false,
                );
                let cur_b_tokens = new_positions.get_collateral(reserve.index);
                let mut to_burn = reserve.to_b_token_up(request.amount);
                let mut tokens_out = request.amount;
                if to_burn > cur_b_tokens {
                    to_burn = cur_b_tokens;
                    tokens_out = reserve.to_asset_from_b_token(cur_b_tokens);
                }
                reserve.b_supply -= to_burn;
                new_positions.remove_collateral(e, &reserve, to_burn);
                actions.set(
                    reserve.asset.clone(),
                    actions
                        .get(request.address.clone())
                        .or(Some(0))
                        .unwrap_optimized()
                        - tokens_out,
                );
                check_health = true;
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "withdraw_collateral"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (tokens_out, to_burn),
                );
            }
            4 => {
                // TODO: do we need to add error logic here for invalid reserve address?
                reserve = pool.load_reserve(e, &request.address);
                // borrow
                emissions::update_emissions(
                    e,
                    reserve.index * 2,
                    reserve.d_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_liabilities(reserve.index),
                    false,
                );
                let d_tokens_minted = reserve.to_d_token_up(request.amount);
                reserve.d_supply += d_tokens_minted;
                reserve.require_utilization_below_max(e);
                new_positions.add_liabilities(e, &reserve, d_tokens_minted);
                actions.set(
                    reserve.asset.clone(),
                    actions
                        .get(request.address.clone())
                        .or(Some(0))
                        .unwrap_optimized()
                        - request.amount,
                );
                check_health = true;
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "borrow"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, d_tokens_minted),
                );
            }
            5 => {
                // TODO: do we need to add error logic here for invalid reserve address?
                reserve = pool.load_reserve(e, &request.address);
                // repay
                emissions::update_emissions(
                    e,
                    reserve.index * 2,
                    reserve.d_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_liabilities(reserve.index),
                    false,
                );
                let cur_d_tokens = new_positions.get_liabilities(reserve.index);
                let d_tokens_burnt = reserve.to_d_token_down(request.amount);
                if d_tokens_burnt > cur_d_tokens {
                    let amount_to_refund =
                        request.amount - reserve.to_asset_from_d_token(cur_d_tokens);
                    require_nonnegative(e, &amount_to_refund);
                    reserve.d_supply -= cur_d_tokens;
                    new_positions.remove_liabilities(e, &reserve, cur_d_tokens);
                    actions.set(
                        reserve.asset.clone(),
                        actions
                            .get(request.address.clone())
                            .or(Some(0))
                            .unwrap_optimized()
                            - amount_to_refund
                            + request.amount,
                    );
                    e.events().publish(
                        (
                            Symbol::new(e, "repay"),
                            request.address.clone().clone(),
                            from.clone(),
                        ),
                        (request.amount - amount_to_refund, cur_d_tokens),
                    );
                } else {
                    reserve.d_supply -= d_tokens_burnt;
                    new_positions.remove_liabilities(e, &reserve, d_tokens_burnt);
                    actions.set(
                        reserve.asset.clone(),
                        actions
                            .get(request.address.clone())
                            .or(Some(0))
                            .unwrap_optimized()
                            + request.amount,
                    );
                    e.events().publish(
                        (
                            Symbol::new(e, "repay"),
                            request.address.clone().clone(),
                            from.clone(),
                        ),
                        (request.amount, d_tokens_burnt),
                    );
                }
                pool.cache_reserve(reserve);
            }
            6 => {
                if request.address == storage::get_backstop(&e) {
                    auctions::fill(e, 1, &request.address, &from);
                    storage::del_auction(e, &1, &request.address);
                } else {
                    auctions::fill(e, 0, &request.address, &from);
                    storage::del_auction(e, &0, &request.address);
                }
            }
            _ => panic_with_error!(e, PoolError::BadRequest),
        }
    }
    (actions, new_positions, check_health)
}

#[cfg(test)]
mod tests {
    use std::println;

    use crate::{storage::PoolConfig, testutils};

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec,
    };

    // d_rate -> 1_000_001_142
    // b_rate -> 1_000_000_686

    /***** supply *****/

    #[test]
    fn test_build_actions_from_request_supply() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 0,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying.clone());
            assert_eq!(action_amount, 10_1234567);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(positions.get_supply(0), 10_1234488);

            let reserve = pool.load_reserve(&e, &underlying.clone());
            assert_eq!(
                reserve.b_supply,
                reserve_data.b_supply + positions.get_supply(0)
            );
        });
    }

    /***** withdraw *****/

    #[test]
    fn test_build_actions_from_request_withdraw() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve = testutils::create_reserve(
            &e,
            &pool,
            &underlying_1.clone(),
            &reserve_config,
            &reserve_data,
        );

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e, &samwise);
        user_positions.add_supply(&e, &reserve, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 1,
                    address: underlying_1.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action_amount, 10_1234567);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(positions.get_supply(0), 9_8765502);

            let reserve = pool.load_reserve(&e, &underlying_1.clone());
            assert_eq!(
                reserve.b_supply,
                reserve_data.b_supply - (20_0000000 - 9_8765502)
            );
        });
    }

    #[test]
    fn test_build_actions_from_request_withdraw_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve = testutils::create_reserve(
            &e,
            &pool,
            &underlying_1.clone(),
            &reserve_config,
            &reserve_data,
        );

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e, &samwise);
        user_positions.add_supply(&e, &reserve, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 1,
                    address: underlying_1.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action_amount, -20_0000137);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying_1.clone());
            assert_eq!(reserve.b_supply, reserve_data.b_supply - 20_0000000);
        });
    }

    /***** supply collateral *****/

    #[test]
    fn test_build_actions_from_request_supply_collateral() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 2,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying.clone());
            assert_eq!(action_amount, 10_1234567);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(positions.get_collateral(0), 10_1234488);

            let reserve = pool.load_reserve(&e, &underlying.clone());
            assert_eq!(
                reserve.b_supply,
                reserve_data.b_supply + positions.get_collateral(0)
            );
        });
    }

    /***** withdraw collateral *****/

    #[test]
    fn test_build_actions_from_request_withdraw_collateral() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve =
            testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e, &samwise);
        user_positions.add_collateral(&e, &reserve, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 3,
                    address: underlying_1.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action_amount, -10_1234567);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(positions.get_collateral(0), 9_8765502);

            let reserve = pool.load_reserve(&e, &underlying_1);
            assert_eq!(
                reserve.b_supply,
                reserve_data.b_supply - (20_0000000 - 9_8765502)
            );
        });
    }

    #[test]
    fn test_build_actions_from_request_withdraw_collateral_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve = testutils::create_reserve(
            &e,
            &pool,
            &underlying_1.clone(),
            &reserve_config,
            &reserve_data,
        );

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e, &samwise);
        user_positions.add_collateral(&e, &reserve, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 3,
                    address: underlying_1.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action_amount, -20_0000137);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying_1.clone());
            assert_eq!(reserve.b_supply, reserve_data.b_supply - 20_0000000);
        });
    }

    /***** borrow *****/

    #[test]
    fn test_build_actions_from_request_borrow() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (_reserve_config, reserve_data) = testutils::default_reserve_meta(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 4,
                    address: underlying_1.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying_1.clone());
            println!("action_amount: {}", action_amount);
            assert_eq!(action_amount, -10_1234567);

            assert_eq!(positions.liabilities.len(), 1);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(positions.get_liabilities(0), 10_1234452);

            let reserve = pool.load_reserve(&e, &underlying_1);
            assert_eq!(reserve.d_supply, reserve_data.d_supply + 10_1234452);
        });
    }

    /***** repay *****/

    #[test]
    fn test_build_actions_from_request_repay() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve =
            testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e, &samwise);
        user_positions.add_liabilities(&e, &reserve, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 5,
                    address: underlying_1.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action_amount, 10_1234567);

            assert_eq!(positions.liabilities.len(), 1);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);
            let d_tokens_repaid = 10_1234451;
            assert_eq!(positions.get_liabilities(0), 20_0000000 - d_tokens_repaid);

            let reserve = pool.load_reserve(&e, &underlying_1);
            assert_eq!(reserve.d_supply, reserve_data.d_supply - d_tokens_repaid);
        });
    }

    #[test]
    fn test_build_actions_from_request_repay_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve = testutils::create_reserve(
            &e,
            &pool,
            &underlying_1.clone(),
            &reserve_config,
            &reserve_data,
        );

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e, &samwise);
        user_positions.add_liabilities(&e, &reserve, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 5,
                    address: underlying_1.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action_amount = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action_amount, 21_0000000 - 0_9999771);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying_1);
            assert_eq!(reserve.d_supply, reserve_data.d_supply - 20_0000000);
        });
    }
}
