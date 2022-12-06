use soroban_auth::Identifier;
use soroban_sdk::Env;

// TODO: Fill out these functions once we have a liquidity pool implementation
pub fn get_lp_share_value(e: &Env) -> u64 {
    1
}

pub fn get_lp_blend_holdings(e: &Env, holder: Identifier) -> u64 {
    let share_value = get_lp_share_value(e);
    let share_holdings = 100;
    share_value * share_holdings
}
