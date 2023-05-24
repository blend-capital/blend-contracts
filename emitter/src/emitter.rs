use crate::{
    constants::SCALAR_7,
    dependencies::{BackstopClient, TokenClient},
    errors::EmitterError,
    storage,
};
use soroban_sdk::{Address, Env};

/// Perform a distribution
pub fn execute_distribute(e: &Env, backstop: &Address) -> Result<i128, EmitterError> {
    let timestamp = e.ledger().timestamp();
    let seconds_since_last_distro = timestamp - storage::get_last_distro_time(e);
    // Blend tokens are distributed at a rate of 1 token per second
    let distribution_amount = (seconds_since_last_distro as i128) * SCALAR_7;
    storage::set_last_distro_time(e, &timestamp);

    let blend_id = storage::get_blend_id(e);
    let blend_client = TokenClient::new(e, &blend_id);
    blend_client.mint(&backstop, &distribution_amount);

    Ok(distribution_amount)
}

/// Perform a backstop swap
pub fn execute_swap_backstop(e: &Env, new_backstop_id: Address) -> Result<(), EmitterError> {
    let backstop = storage::get_backstop(e);
    let backstop_token = BackstopClient::new(&e, &backstop).bstp_token();
    let backstop_token_client = TokenClient::new(&e, &backstop_token);

    let backstop_balance = backstop_token_client.balance(&backstop);
    let new_backstop_balance = backstop_token_client.balance(&new_backstop_id);
    if new_backstop_balance > backstop_balance {
        storage::set_backstop(e, &new_backstop_id);
        Ok(())
    } else {
        return Err(EmitterError::InsufficientBackstopSize);
    }
}
