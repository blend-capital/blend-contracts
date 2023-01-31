use fixed_point_math::FixedPoint;
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env};

use crate::{
    constants::SCALAR_7,
    user_data::{UserAction, UserData},
};

/// Validate if a user is currently healthy given an incoming actions.
///
/// ### Arguments
/// * `oracle` - The oracle address
/// * `user` - The user to check
/// * `user_action` - An incoming user action
pub fn validate_hf(
    e: &Env,
    oracle: &BytesN<32>,
    user: &Identifier,
    user_action: &UserAction,
) -> bool {
    let account_data = UserData::load(e, oracle, user, &user_action);
    // Note: User is required to have at least 5% excess collateral in order to undertake an action that would reduce their health factor
    let collateral_required = account_data
        .liability_base
        .clone()
        .fixed_mul_ceil(1_0500000, SCALAR_7)
        .unwrap();
    return (collateral_required < account_data.collateral_base)
        || (account_data.liability_base == 0);
}
