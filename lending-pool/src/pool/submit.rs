use crate::{dependencies::TokenClient, storage};
use soroban_sdk::{Address, Env, Vec};

use super::{
    actions::{build_actions_from_request, Request},
    health_factor::PositionData,
    pool::Pool,
    Positions,
};

/// Execute a set of updates for a user against the pool.
///
/// ### Arguments
/// * from - The address of the user whose positions are being modified
/// * spender - The address of the user who is sending tokens to the pool
/// * to - The address of the user who is receiving tokens from the pool
/// * requests - A vec of requests to be processed
///
/// ### Panics
/// If the request is unable to be fully executed
pub fn execute_submit(
    e: &Env,
    from: &Address,
    spender: &Address,
    to: &Address,
    requests: Vec<Request>,
) -> Positions {
    let mut pool = Pool::load(e);

    let (pool_actions, new_positions, check_health) =
        build_actions_from_request(e, &mut pool, &from, requests);

    if check_health {
        // panics if the new positions set does not meet the health factor requirement
        PositionData::calculate_from_positions(e, &mut pool, &new_positions).require_healthy(e);
    }

    // TODO: Is this reentrancy guard necessary?
    // transfer tokens into the pool
    for action in pool_actions.iter_unchecked() {
        if action.tokens_in > 0 {
            TokenClient::new(e, &action.asset).transfer(
                &spender,
                &e.current_contract_address(),
                &action.tokens_in,
            );
        }
    }

    // store updated info to ledger
    pool.store_cached_reserves(e);
    storage::set_user_positions(e, &from, &new_positions);

    // transfer tokens out of the pool
    for action in pool_actions.iter_unchecked() {
        if action.tokens_out > 0 {
            TokenClient::new(e, &action.asset).transfer(
                &e.current_contract_address(),
                &to,
                &action.tokens_out,
            );
        }
    }

    new_positions
}
