use cast::u32;
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
/// An action the pool takes as result of requests
#[derive(Clone)]
#[contracttype]
pub struct Action {
    pub tokens_in: i128,
    pub tokens_out: i128,
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
) -> (Map<Address, Action>, Positions, bool) {
    let mut actions: Map<Address, Action> = Map::new(&e); //tokens in is positive, tokens out is negative
    let old_positions = storage::get_user_positions(e, from);
    let mut new_positions = old_positions.clone();
    let mut check_health = false;
    for request in requests.iter() {
        // verify reserve is supported in the pool and the action is allowed
        require_nonnegative(e, &request.amount);
        pool.require_action_allowed(e, request.request_type.clone());
        let mut reserve: Reserve;

        match request.request_type {
            0 => {
                reserve = pool.load_reserve(e, &request.address);
                // supply
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                reserve.b_supply += b_tokens_minted;
                new_positions.add_supply(e, &reserve, b_tokens_minted);
                if let Some(action) = actions.get(reserve.asset.clone()) {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: action.tokens_in + request.amount,
                            tokens_out: action.tokens_out,
                        },
                    );
                } else {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: request.amount,
                            tokens_out: 0,
                        },
                    );
                }
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(&e, "supply"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, b_tokens_minted),
                );
            }
            1 => {
                reserve = pool.load_reserve(e, &request.address);
                // withdraw
                let cur_b_tokens = new_positions.get_supply(reserve.index);
                let mut to_burn = reserve.to_b_token_up(request.amount);
                let mut tokens_out = request.amount;
                if to_burn > cur_b_tokens {
                    to_burn = cur_b_tokens;
                    tokens_out = reserve.to_asset_from_b_token(cur_b_tokens);
                }
                reserve.b_supply -= to_burn;
                new_positions.remove_supply(e, &reserve, to_burn);
                if let Some(action) = actions.get(reserve.asset.clone()) {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: action.tokens_in,
                            tokens_out: action.tokens_out + tokens_out,
                        },
                    );
                } else {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: 0,
                            tokens_out: tokens_out,
                        },
                    );
                }

                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(&e, "withdraw"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (tokens_out, to_burn),
                );
            }
            2 => {
                reserve = pool.load_reserve(e, &request.address);
                // supply collateral
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                reserve.b_supply += b_tokens_minted;
                new_positions.add_collateral(e, &reserve, b_tokens_minted);
                if let Some(action) = actions.get(reserve.asset.clone()) {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: action.tokens_in + request.amount,
                            tokens_out: action.tokens_out,
                        },
                    );
                } else {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: request.amount,
                            tokens_out: 0,
                        },
                    );
                }
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(&e, "supply_collateral"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, b_tokens_minted),
                );
            }
            3 => {
                reserve = pool.load_reserve(e, &request.address);
                // withdraw collateral
                let cur_b_tokens = new_positions.get_collateral(reserve.index);
                let mut to_burn = reserve.to_b_token_up(request.amount);
                let mut tokens_out = request.amount;
                if to_burn > cur_b_tokens {
                    to_burn = cur_b_tokens;
                    tokens_out = reserve.to_asset_from_b_token(cur_b_tokens);
                }
                reserve.b_supply -= to_burn;
                new_positions.remove_collateral(e, &reserve, to_burn);

                if let Some(action) = actions.get(reserve.asset.clone()) {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: action.tokens_in,
                            tokens_out: action.tokens_out + tokens_out,
                        },
                    );
                } else {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: 0,
                            tokens_out: tokens_out,
                        },
                    );
                }
                check_health = true;
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(&e, "withdraw_collateral"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (tokens_out, to_burn),
                );
            }
            4 => {
                reserve = pool.load_reserve(e, &request.address);
                // borrow
                let d_tokens_minted = reserve.to_d_token_up(request.amount);
                reserve.d_supply += d_tokens_minted;
                reserve.require_utilization_below_max(e);
                new_positions.add_liabilities(e, &reserve, d_tokens_minted);
                if let Some(action) = actions.get(reserve.asset.clone()) {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: action.tokens_in,
                            tokens_out: action.tokens_out + request.amount,
                        },
                    );
                } else {
                    actions.set(
                        reserve.asset.clone(),
                        Action {
                            tokens_in: 0,
                            tokens_out: request.amount,
                        },
                    );
                }
                check_health = true;
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(&e, "borrow"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, d_tokens_minted),
                );
            }
            5 => {
                reserve = pool.load_reserve(e, &request.address);
                // repay
                let cur_d_tokens = new_positions.get_liabilities(reserve.index);
                let d_tokens_burnt = reserve.to_d_token_down(request.amount);
                if d_tokens_burnt > cur_d_tokens {
                    let amount_to_refund =
                        request.amount - reserve.to_asset_from_d_token(cur_d_tokens);
                    require_nonnegative(e, &amount_to_refund);
                    reserve.d_supply -= cur_d_tokens;
                    new_positions.remove_liabilities(e, &reserve, cur_d_tokens);

                    if let Some(action) = actions.get(reserve.asset.clone()) {
                        actions.set(
                            reserve.asset.clone(),
                            Action {
                                tokens_in: action.tokens_in + request.amount,
                                tokens_out: action.tokens_out + amount_to_refund,
                            },
                        );
                    } else {
                        actions.set(
                            reserve.asset.clone(),
                            Action {
                                tokens_in: request.amount,
                                tokens_out: amount_to_refund,
                            },
                        );
                    }
                    e.events().publish(
                        (
                            Symbol::new(&e, "repay"),
                            request.address.clone().clone(),
                            from.clone(),
                        ),
                        (request.amount - amount_to_refund, cur_d_tokens),
                    );
                } else {
                    reserve.d_supply -= d_tokens_burnt;
                    new_positions.remove_liabilities(e, &reserve, d_tokens_burnt);

                    if let Some(action) = actions.get(reserve.asset.clone()) {
                        actions.set(
                            reserve.asset.clone(),
                            Action {
                                tokens_in: action.tokens_in + request.amount,
                                tokens_out: action.tokens_out,
                            },
                        );
                    } else {
                        actions.set(
                            reserve.asset.clone(),
                            Action {
                                tokens_in: request.amount,
                                tokens_out: 0,
                            },
                        );
                    }
                    e.events().publish(
                        (
                            Symbol::new(&e, "repay"),
                            request.address.clone().clone(),
                            from.clone(),
                        ),
                        (request.amount, d_tokens_burnt),
                    );
                }
                pool.cache_reserve(reserve);
            }
            6 => {
                auctions::fill(
                    e,
                    u32(request.amount).unwrap_optimized(),
                    &request.address,
                    &from,
                );
                if request.amount < 2 {
                    check_health = true;
                }
                e.events().publish(
                    (
                        Symbol::new(&e, "fill_auction"),
                        request.address.clone().clone(),
                        request.amount,
                    ),
                    from.clone(),
                );
            }
            _ => panic_with_error!(e, PoolError::BadRequest),
        }
    }
    (actions, new_positions, check_health)
}

#[cfg(test)]
mod tests {
    use std::println;

    use crate::{storage::PoolConfig, testutils, AuctionData, AuctionType};

    use super::*;
    use soroban_sdk::{
        map,
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
            let action = actions.get_unchecked(underlying.clone());
            assert_eq!(action.tokens_in, 10_1234567);
            assert_eq!(action.tokens_out, 0);

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

        e.as_contract(&pool, || {
            let mut user_positions = Positions::env_default(&e, &samwise);
            user_positions.add_supply(&e, &reserve, 20_0000000);
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
            println!("about to build actions");
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action.tokens_out, 10_1234567);
            assert_eq!(action.tokens_in, 0);

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
        e.as_contract(&pool, || {
            user_positions.add_supply(&e, &reserve, 20_0000000);
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
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action.tokens_out, 20_0000137);
            assert_eq!(action.tokens_in, 0);
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
            let action = actions.get_unchecked(underlying.clone());
            assert_eq!(action.tokens_in, 10_1234567);
            assert_eq!(action.tokens_out, 0);

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
        e.as_contract(&pool, || {
            user_positions.add_collateral(&e, &reserve, 20_0000000);
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
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action.tokens_out, 10_1234567);
            assert_eq!(action.tokens_in, 0);

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
        e.as_contract(&pool, || {
            user_positions.add_collateral(&e, &reserve, 20_0000000);
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
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action.tokens_out, 20_0000137);
            assert_eq!(action.tokens_in, 0);

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
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(
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
            println!("here");
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);
            println!("here");
            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action.tokens_out, 10_1234567);
            assert_eq!(action.tokens_in, 0);

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
        e.as_contract(&pool, || {
            user_positions.add_liabilities(&e, &reserve, 20_0000000);
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
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action.tokens_in, 10_1234567);
            assert_eq!(action.tokens_out, 0);

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
        e.as_contract(&pool, || {
            user_positions.add_liabilities(&e, &reserve, 20_0000000);
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
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(action.tokens_in, 21_0000000);
            assert_eq!(action.tokens_out, 0_9999771);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying_1);
            assert_eq!(reserve.d_supply, reserve_data.d_supply - 20_0000000);
        });
    }
    #[test]
    fn test_aggregating_actions() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 600;
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
        let user_positions = Positions::env_default(&e, &samwise);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 0,
                    address: underlying_1.clone(),
                    amount: 10_0000000,
                },
                Request {
                    request_type: 1,
                    address: underlying_1.clone(),
                    amount: 5_0000000,
                },
                Request {
                    request_type: 2,
                    address: underlying_1.clone(),
                    amount: 10_0000000,
                },
                Request {
                    request_type: 3,
                    address: underlying_1.clone(),
                    amount: 5_0000000,
                },
                Request {
                    request_type: 4,
                    address: underlying_1.clone(),
                    amount: 20_0000000,
                },
                Request {
                    request_type: 5,
                    address: underlying_1.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(underlying_1.clone());
            assert_eq!(
                action.tokens_out,
                5_0000000 + 5_0000000 + 20_0000000 + 1_0000000
            );
            assert_eq!(action.tokens_in, 10_0000000 + 10_0000000 + 21_0000000);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(positions.collateral.get_unchecked(reserve.index), 5_0000000);
            assert_eq!(positions.supply.get_unchecked(reserve.index), 5_0000000);
        });
    }
    #[test]
    fn test_fill_user_liquidation() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 176 + 200,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta(&e);
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
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
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta(&e);
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
            user: samwise.clone(),
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 6,
                    address: samwise.clone(),
                    amount: 0,
                },
            ];
            let (actions, _, health_check) =
                build_actions_from_request(&e, &mut pool, &frodo, requests);

            assert_eq!(health_check, true);
            assert_eq!(
                storage::has_auction(&e, &(AuctionType::UserLiquidation as u32), &samwise),
                false
            );
            assert_eq!(actions.len(), 0);
        });
    }
    #[test]
    fn test_fill_bad_debt_auction() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 51 + 200,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let pool_address = Address::random(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
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
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta(&e);
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
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let auction_data = AuctionData {
            bid: map![&e, (underlying_0, 10_0000000), (underlying_1, 2_5000000)],
            lot: map![&e, (backstop_token_id, 95_2000000)],
            block: 51,
        };
        let positions: Positions = Positions {
            user: backstop_address.clone(),
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
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &backstop_address, &positions);
            storage::set_auction(
                &e,
                &(AuctionType::BadDebtAuction as u32),
                &backstop_address,
                &auction_data,
            );

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 6,
                    address: backstop_address.clone(),
                    amount: 1,
                },
            ];
            let (actions, _, health_check) =
                build_actions_from_request(&e, &mut pool, &frodo, requests);

            assert_eq!(health_check, true);
            assert_eq!(
                storage::has_auction(&e, &(AuctionType::BadDebtAuction as u32), &backstop_address),
                false
            );
            assert_eq!(actions.len(), 0);
        });
    }
    #[test]
    fn test_fill_interest_auction() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 51 + 200,
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

        e.budget().reset_unlimited();
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
        let auction_data = AuctionData {
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
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 6,
                    address: backstop_address.clone(),
                    amount: 2,
                },
            ];
            let (actions, _, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);
            assert_eq!(
                storage::has_auction(
                    &e,
                    &(AuctionType::InterestAuction as u32),
                    &backstop_address
                ),
                false
            );
            assert_eq!(actions.len(), 0);
        });
    }
}
