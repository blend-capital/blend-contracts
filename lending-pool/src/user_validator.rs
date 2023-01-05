use soroban_auth::Identifier;
use soroban_sdk::Env;

use crate::user_data::{UserAction, UserData};

/// Validate if a user is currently healthy given an incoming actions.
///
/// ### Arguments
/// * `user` - The user to check
/// * `user_action` - An incoming user action
pub fn validate_hf(e: &Env, user: &Identifier, user_action: &UserAction) -> bool {
    let account_data = UserData::load(e, user, &user_action);
    // Note: User is required to have more 5% excess collateral in order to undertake an action that would reduce their health factor
    let collateral_required = (account_data.e_liability_base.clone() * 1_0500000) / 1_0000000;
    return (collateral_required < account_data.e_collateral_base)
        || (account_data.e_liability_base == 0);
}
