use crate::{
    constants::SCALAR_7, dependencies::TokenClient, errors::EmitterError,
    lp_reader::get_lp_blend_holdings, storage,
};
use soroban_sdk::{Address, Env};

/// Perform a distribution
pub fn execute_distribute(e: &Env, backstop: &Address) -> Result<i128, EmitterError> {
    let timestamp = e.ledger().timestamp();
    let seconds_since_last_distro = timestamp - storage::get_last_distro_time(e);
    // Blend tokens are distributed at a rate of 1 token per second
    let distribution_amount = (seconds_since_last_distro as i128) * SCALAR_7;

    let blend_id = storage::get_blend_id(e);
    let blend_client = TokenClient::new(e, &blend_id);
    blend_client.mint(
        &e.current_contract_address(),
        &backstop,
        &distribution_amount,
    );

    Ok(distribution_amount)
}

/// Perform a backstop swap
pub fn execute_swap_backstop(e: &Env, new_backstop: Address) -> Result<(), EmitterError> {
    let blend_id = storage::get_blend_id(e);
    let blend_client = TokenClient::new(e, &blend_id);

    let old_backstop = storage::get_backstop(e);
    let old_backstop_blend_balance = blend_client.balance(&old_backstop);
    let old_backstop_blend_lp_balance = get_lp_blend_holdings(&e, old_backstop.clone());
    let effective_old_backstop_blend =
        (old_backstop_blend_balance / 4) + old_backstop_blend_lp_balance;

    let new_backstop_blend_balance = blend_client.balance(&new_backstop);
    let new_backstop_blend_lp_balance = get_lp_blend_holdings(&e, new_backstop.clone());
    let effective_new_backstop_blend =
        (new_backstop_blend_balance / 4) + new_backstop_blend_lp_balance;

    if effective_new_backstop_blend <= effective_old_backstop_blend {
        return Err(EmitterError::InsufficientBLND);
    }

    storage::set_backstop(e, &new_backstop);
    Ok(())
}
