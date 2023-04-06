use crate::{dependencies::TokenClient, errors::BackstopError, pool::Pool, storage, user::User};
use soroban_sdk::{Address, BytesN, Env};

/// Perform a claim by a pool from the backstop module
pub fn execute_claim(
    e: &Env,
    pool_address: &BytesN<32>,
    to: &Address,
    amount: i128,
) -> Result<(), BackstopError> {
    let mut pool = Pool::new(e, pool_address.clone());
    pool.verify_pool(&e)?;
    pool.claim(e, amount)?;
    pool.write_emissions(&e);

    let backstop_token = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token.xfer(&e.current_contract_address(), &to, &amount);

    Ok(())
}
