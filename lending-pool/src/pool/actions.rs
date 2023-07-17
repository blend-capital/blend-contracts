use fixed_point_math::FixedPoint;
use soroban_sdk::{
    contracttype, panic_with_error, unwrap::UnwrapOptimized, vec, Address, Env, Map, Vec,
};

use crate::{
    auctions::{self, AuctionData},
    constants::SCALAR_7,
    dependencies::BackstopClient,
    emissions,
    errors::PoolError,
    pool::Positions,
    storage,
    validator::require_nonnegative,
};

use super::pool::Pool;

/// An request a user makes against the pool
#[derive(Clone)]
#[contracttype]
pub struct Request {
    pub request_type: u32,
    pub reserve_index: u32,
    pub amount: i128,
    pub target: Address,
}

/// A token action to be taken by the pool
#[derive(Clone)]
#[contracttype]
pub struct Action {
    pub tokens_out: i128,
    pub tokens_in: i128,
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
) -> (bool, Map<Address, Action>) {
    let mut actions: Map<Address, Action> = Map::new(&e);
    let mut new_positions = storage::get_user_positions(e, &from);
    let mut old_positions = new_positions.clone();
    let mut check_health = false;
    let reserve_list = storage::get_res_list(e);
    let mut auction_data: AuctionData;
    let (mut lot_modifier, mut bid_modifier): (i128, i128);
    for request in requests.iter_unchecked() {
        // verify reserve is supported in the pool and the action is allowed
        require_nonnegative(e, &request.amount);
        let asset = reserve_list
            .get(request.reserve_index)
            .unwrap_or_else(|| panic_with_error!(e, PoolError::BadRequest))
            .unwrap_optimized();
        pool.require_action_allowed(e, request.request_type);
        let mut reserve = pool.load_reserve(e, &asset);

        if request.target != from.clone() {
            // gets auction data - will panic if there is no auction
            auction_data = storage::get_auction(e, &0, &request.target);
            //store old positions
            storage::set_user_positions(e, &from, &new_positions);
            // re-assign user
            let from = request.target;
            // re-assign positions
            new_positions = storage::get_user_positions(e, &from);
            old_positions = new_positions.clone();
            //get modifiers
            (lot_modifier, bid_modifier) = auctions::get_fill_modifiers(e, &auction_data)
        }

        match request.request_type {
            0 => {
                // supply
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.b_supply,
                    reserve.scalar,
                    &request.target,
                    old_positions.get_total_supply(request.reserve_index),
                    false,
                );
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                reserve.b_supply += b_tokens_minted;
                new_positions.add_supply(request.reserve_index, b_tokens_minted);
                let mut new_action: Action;
                if actions.contains_key(asset.clone()) {
                    new_action = actions
                        .get(asset.clone())
                        .unwrap_optimized()
                        .unwrap_optimized();
                    new_action.tokens_out += 0;
                    new_action.tokens_in += request.amount;
                } else {
                    new_action = Action {
                        tokens_out: 0,
                        tokens_in: request.amount,
                    };
                }
                actions.set(asset.clone(), new_action);
            }
            1 => {
                // withdraw
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(request.reserve_index),
                    false,
                );
                let cur_b_tokens = new_positions.get_supply(request.reserve_index);
                let b_tokens_burnt = reserve.to_b_token_up(request.amount);
                if b_tokens_burnt > cur_b_tokens {
                    let amount = reserve.to_asset_from_b_token(cur_b_tokens);
                    reserve.b_supply -= cur_b_tokens;
                    new_positions.remove_supply(e, request.reserve_index, cur_b_tokens);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += amount;
                        new_action.tokens_in += 0;
                    } else {
                        new_action = Action {
                            tokens_out: amount,
                            tokens_in: 0,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                } else {
                    reserve.b_supply -= b_tokens_burnt;
                    new_positions.remove_supply(e, request.reserve_index, b_tokens_burnt);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += request.amount;
                        new_action.tokens_in += 0;
                    } else {
                        new_action = Action {
                            tokens_out: request.amount,
                            tokens_in: 0,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                }
            }
            2 => {
                // supply collateral
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(request.reserve_index),
                    false,
                );
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                reserve.b_supply += b_tokens_minted;
                new_positions.add_collateral(request.reserve_index, b_tokens_minted);
                let mut new_action: Action;
                if actions.contains_key(asset.clone()) {
                    new_action = actions
                        .get(asset.clone())
                        .unwrap_optimized()
                        .unwrap_optimized();
                    new_action.tokens_out += 0;
                    new_action.tokens_in += request.amount;
                } else {
                    new_action = Action {
                        tokens_out: 0,
                        tokens_in: request.amount,
                    };
                }
                actions.set(asset.clone(), new_action);
            }
            3 => {
                // withdraw collateral
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(request.reserve_index),
                    false,
                );
                let cur_b_tokens = new_positions.get_collateral(request.reserve_index);
                let b_tokens_burnt = reserve.to_b_token_up(request.amount);
                if b_tokens_burnt > cur_b_tokens {
                    let amount = reserve.to_asset_from_b_token(cur_b_tokens);
                    reserve.b_supply -= cur_b_tokens;
                    new_positions.remove_collateral(e, request.reserve_index, cur_b_tokens);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += amount;
                        new_action.tokens_in += 0;
                    } else {
                        new_action = Action {
                            tokens_out: amount,
                            tokens_in: 0,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                } else {
                    reserve.b_supply -= b_tokens_burnt;
                    new_positions.remove_collateral(e, request.reserve_index, b_tokens_burnt);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += request.amount;
                        new_action.tokens_in += 0;
                    } else {
                        new_action = Action {
                            tokens_out: request.amount,
                            tokens_in: 0,
                        };
                    }
                    new_action.tokens_out += request.amount;
                    new_action.tokens_in += 0;
                    actions.set(asset.clone(), new_action);
                }
                check_health = true;
            }
            4 => {
                // borrow
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.d_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_liabilities(request.reserve_index),
                    false,
                );
                let d_tokens_minted = reserve.to_d_token_up(request.amount);
                reserve.d_supply += d_tokens_minted;
                reserve.require_utilization_below_max(e);
                new_positions.add_liabilities(request.reserve_index, d_tokens_minted);
                let mut new_action: Action;
                if actions.contains_key(asset.clone()) {
                    new_action = actions
                        .get(asset.clone())
                        .unwrap_optimized()
                        .unwrap_optimized();
                    new_action.tokens_out += request.amount;
                    new_action.tokens_in += 0;
                } else {
                    new_action = Action {
                        tokens_out: request.amount,
                        tokens_in: 0,
                    };
                }
                actions.set(asset.clone(), new_action);
                check_health = true;
            }
            5 => {
                // repay
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.d_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_liabilities(request.reserve_index),
                    false,
                );
                let cur_d_tokens = new_positions.get_liabilities(request.reserve_index);
                let d_tokens_burnt = reserve.to_d_token_down(request.amount);
                if d_tokens_burnt > cur_d_tokens {
                    let amount_to_refund =
                        request.amount - reserve.to_asset_from_d_token(cur_d_tokens);
                    require_nonnegative(e, &amount_to_refund);
                    reserve.d_supply -= cur_d_tokens;
                    new_positions.remove_liabilities(e, request.reserve_index, cur_d_tokens);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += amount_to_refund;
                        new_action.tokens_in += request.amount;
                    } else {
                        new_action = Action {
                            tokens_out: amount_to_refund,
                            tokens_in: request.amount,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                } else {
                    reserve.d_supply -= d_tokens_burnt;
                    new_positions.remove_liabilities(e, request.reserve_index, d_tokens_burnt);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += 0;
                        new_action.tokens_in += request.amount;
                    } else {
                        new_action = Action {
                            tokens_out: 0,
                            tokens_in: request.amount,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                }
            }
            6 => {
                // check auction - will error if auction data has not been assigned yet
                if auction_data.assets.len() < 1 {
                    panic_with_error!(&e, PoolError::BadRequest)
                }
                // withdraw collateral liquidation
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.b_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_total_supply(request.reserve_index),
                    false,
                );
                let cur_b_tokens = new_positions.get_collateral(request.reserve_index);
                let b_tokens_burnt = reserve.to_b_token_up(request.amount);
                if cur_b_tokens
                    < b_tokens_burnt
                        .fixed_mul_ceil(lot_modifier, SCALAR_7)
                        .unwrap_optimized()
                {
                    panic_with_error!(&e, PoolError::InvalidLiquidation)
                }
                if b_tokens_burnt > cur_b_tokens {
                    let amount = reserve.to_asset_from_b_token(cur_b_tokens);
                    reserve.b_supply -= cur_b_tokens;
                    new_positions.remove_collateral(e, request.reserve_index, cur_b_tokens);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += amount;
                        new_action.tokens_in += 0;
                    } else {
                        new_action = Action {
                            tokens_out: amount,
                            tokens_in: 0,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                } else {
                    reserve.b_supply -= b_tokens_burnt;
                    new_positions.remove_collateral(e, request.reserve_index, b_tokens_burnt);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += request.amount;
                        new_action.tokens_in += 0;
                    } else {
                        new_action = Action {
                            tokens_out: request.amount,
                            tokens_in: 0,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                }
            }
            7 => {
                // check auction - will error if auction data has not been assigned yet
                if auction_data.assets.len() < 1 {
                    panic_with_error!(&e, PoolError::BadRequest)
                }
                // repay
                emissions::update_emissions(
                    e,
                    request.reserve_index * 2,
                    reserve.d_supply,
                    reserve.scalar,
                    from,
                    old_positions.get_liabilities(request.reserve_index),
                    false,
                );
                let cur_d_tokens = new_positions.get_liabilities(request.reserve_index);
                let d_tokens_burnt = reserve.to_d_token_down(request.amount);

                let bid_amt = auction_data
                    .assets
                    .get(request.reserve_index)
                    .unwrap_or_else(|| panic_with_error!(e, PoolError::BadRequest))
                    .unwrap_optimized();

                if d_tokens_burnt
                    > bid_modifier
                        .fixed_mul_floor(bid_amt, SCALAR_7)
                        .unwrap_optimized()
                {
                    panic_with_error!(&e, PoolError::InvalidLiquidation)
                }
                if d_tokens_burnt > cur_d_tokens {
                    let amount_to_refund =
                        request.amount - reserve.to_asset_from_d_token(cur_d_tokens);
                    require_nonnegative(e, &amount_to_refund);
                    reserve.d_supply -= cur_d_tokens;
                    new_positions.remove_liabilities(e, request.reserve_index, cur_d_tokens);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += amount_to_refund;
                        new_action.tokens_in += request.amount;
                    } else {
                        new_action = Action {
                            tokens_out: amount_to_refund,
                            tokens_in: request.amount,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                } else {
                    reserve.d_supply -= d_tokens_burnt;
                    new_positions.remove_liabilities(e, request.reserve_index, d_tokens_burnt);
                    let mut new_action: Action;
                    if actions.contains_key(asset.clone()) {
                        new_action = actions
                            .get(asset.clone())
                            .unwrap_optimized()
                            .unwrap_optimized();
                        new_action.tokens_out += 0;
                        new_action.tokens_in += request.amount;
                    } else {
                        new_action = Action {
                            tokens_out: 0,
                            tokens_in: request.amount,
                        };
                    }
                    actions.set(asset.clone(), new_action);
                }
            }
            _ => panic_with_error!(e, PoolError::BadRequest),
        }
        pool.cache_reserve(reserve);
    }
    storage::set_user_positions(e, from, &new_positions);

    (check_health, actions)
}
#[cfg(test)]
mod tests {
    use crate::{storage::PoolConfig, testutils};

    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

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
                    reserve_index: 0,
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying);
            assert_eq!(action.tokens_out, 0);
            assert_eq!(action.tokens_in, 10_1234567);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(positions.get_supply(0), 10_1234488);

            let reserve = pool.load_reserve(&e, &underlying);
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
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e);
        user_positions.add_supply(0, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 1,
                    reserve_index: 0,
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying_1);
            assert_eq!(action.tokens_out, 10_1234567);
            assert_eq!(action.tokens_in, 0);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(positions.get_supply(0), 9_8765502);

            let reserve = pool.load_reserve(&e, &underlying_1);
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
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e);
        user_positions.add_supply(0, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 1,
                    reserve_index: 0,
                    amount: 21_0000000,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying_1);
            assert_eq!(action.tokens_out, 20_0000137);
            assert_eq!(action.tokens_in, 0);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying_1);
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
                    reserve_index: 0,
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying);
            assert_eq!(action.tokens_out, 0);
            assert_eq!(action.tokens_in, 10_1234567);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(positions.get_collateral(0), 10_1234488);

            let reserve = pool.load_reserve(&e, &underlying);
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
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e);
        user_positions.add_collateral(0, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 3,
                    reserve_index: 0,
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying_1);
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
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e);
        user_positions.add_collateral(0, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 3,
                    reserve_index: 0,
                    amount: 21_0000000,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying_1);
            assert_eq!(action.tokens_out, 20_0000137);
            assert_eq!(action.tokens_in, 0);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying_1);
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
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
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
                    reserve_index: 0,
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying_1);
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
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e);
        user_positions.add_liabilities(0, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 5,
                    reserve_index: 0,
                    amount: 10_1234567,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying_1);
            assert_eq!(action.tokens_out, 0);
            assert_eq!(action.tokens_in, 10_1234567);

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
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        let mut user_positions = Positions::env_default(&e);
        user_positions.add_liabilities(0, 20_0000000);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: 5,
                    reserve_index: 0,
                    amount: 21_0000000,
                },
            ];
            let (actions, positions, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            assert_eq!(actions.len(), 1);
            let action = actions.get_unchecked(0).unwrap_optimized();
            assert_eq!(action.asset, underlying_1);
            assert_eq!(action.tokens_out, 0_9999771);
            assert_eq!(action.tokens_in, 21_0000000);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying_1);
            assert_eq!(reserve.d_supply, reserve_data.d_supply - 20_0000000);
        });
    }
}
