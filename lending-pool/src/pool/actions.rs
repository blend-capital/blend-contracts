use soroban_sdk::{contracttype, panic_with_error, vec, Address, Env, Vec, unwrap::UnwrapOptimized};

use crate::{
    emissions, errors::PoolError, pool::Positions, storage, validator::require_nonnegative,
};

use super::pool::Pool;

/// An request a user makes against the pool
#[derive(Clone)]
#[contracttype]
pub struct Request {
    pub request_type: u32,
    pub reserve_index: u32,
    pub amount: i128,
}

// A token action to be taken by the pool
#[derive(Clone)]
#[contracttype]
pub struct Action {
    pub asset: Address,
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
) -> (Vec<Action>, Positions, bool) {
    let mut actions = vec![&e];
    let old_positions = storage::get_user_positions(e, from);
    let mut new_positions = old_positions.clone();
    let mut check_health = false;
    let reserve_list = storage::get_res_list(e);
    for request in requests.iter_unchecked() {
        // verify reserve is supported in the pool and the action is allowed
        require_nonnegative(e, &request.amount);
        let asset = reserve_list
            .get(request.reserve_index)
            .unwrap_or_else(|| panic_with_error!(e, PoolError::BadRequest))
            .unwrap_optimized();
        pool.require_action_allowed(e, request.request_type);
        let mut reserve = pool.load_reserve(e, &asset);

        match request.request_type {
            0 => {
                // supply
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
                new_positions.add_supply(request.reserve_index, b_tokens_minted);
                actions.push_back(Action {
                    asset: asset.clone(),
                    tokens_out: 0,
                    tokens_in: request.amount,
                });
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
                    actions.push_back(Action {
                        asset: asset.clone(),
                        tokens_out: amount,
                        tokens_in: 0,
                    });
                } else {
                    reserve.b_supply -= b_tokens_burnt;
                    new_positions.remove_supply(e, request.reserve_index, b_tokens_burnt);
                    actions.push_back(Action {
                        asset: asset.clone(),
                        tokens_out: request.amount,
                        tokens_in: 0,
                    });
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
                actions.push_back(Action {
                    asset: asset.clone(),
                    tokens_out: 0,
                    tokens_in: request.amount,
                });
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
                    actions.push_back(Action {
                        asset: asset.clone(),
                        tokens_out: amount,
                        tokens_in: 0,
                    });
                } else {
                    reserve.b_supply -= b_tokens_burnt;
                    new_positions.remove_collateral(e, request.reserve_index, b_tokens_burnt);
                    actions.push_back(Action {
                        asset: asset.clone(),
                        tokens_out: request.amount,
                        tokens_in: 0,
                    });
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
                actions.push_back(Action {
                    asset: asset.clone(),
                    tokens_out: request.amount,
                    tokens_in: 0,
                });
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
                    actions.push_back(Action {
                        asset: asset.clone(),
                        tokens_out: amount_to_refund,
                        tokens_in: request.amount,
                    });
                } else {
                    reserve.d_supply -= d_tokens_burnt;
                    new_positions.remove_liabilities(e, request.reserve_index, d_tokens_burnt);
                    actions.push_back(Action {
                        asset: asset.clone(),
                        tokens_out: 0,
                        tokens_in: request.amount,
                    });
                }
            }
            _ => panic_with_error!(e, PoolError::BadRequest),
        }
        pool.cache_reserve(reserve);
    }
    (actions, new_positions, check_health)
}
