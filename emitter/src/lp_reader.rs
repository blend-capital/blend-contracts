use soroban_auth::Identifier;
use soroban_sdk::{BigInt, Env};

//TODO: Fill out these functions once we have a liquidity pool implementation
pub fn get_lp_share_value(e: &Env) -> BigInt {
    BigInt::from_i64(&e, 1)
}

pub fn get_lp_blend_holdings(e: &Env, holder: Identifier) -> BigInt {
    let share_value = get_lp_share_value(e);
    let share_holdings = BigInt::from_i64(&e, 100);
    share_value * share_holdings
}
