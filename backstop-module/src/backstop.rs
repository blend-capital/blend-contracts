use crate::{
    constants::BACKSTOP_TOKEN, dependencies::TokenClient, errors::BackstopError, pool::Pool,
    storage, user::User,
};
use soroban_sdk::{Address, BytesN, Env};

/// Perform a deposit into the backstop module
pub fn execute_deposit(
    e: &Env,
    from: Address,
    pool_address: BytesN<32>,
    amount: i128,
) -> Result<i128, BackstopError> {
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    let to_mint = pool.convert_to_shares(e, amount);

    let backstop_token_client = TokenClient::new(e, &BytesN::from_array(e, &BACKSTOP_TOKEN));
    backstop_token_client.xfer_from(
        &e.current_contract_address(),
        &from,
        &e.current_contract_address(),
        &amount,
    );

    // "mint" shares
    pool.deposit(e, amount, to_mint);
    pool.write_shares(e);
    pool.write_tokens(e);

    user.add_shares(e, to_mint);
    user.write_shares(e);

    Ok(to_mint)
}

/// Perform a queue for withdraw from the backstop module
pub fn execute_q_withdraw(
    e: &Env,
    from: Address,
    pool_address: BytesN<32>,
    amount: i128,
) -> Result<storage::Q4W, BackstopError> {
    let mut user = User::new(pool_address.clone(), from);
    let mut pool = Pool::new(e, pool_address.clone());

    let new_q4w = user.try_queue_shares_for_withdrawal(e, amount)?;
    user.write_q4w(&e);

    pool.queue_for_withdraw(e, amount);
    pool.write_q4w(&e);

    Ok(new_q4w)
}

/// Perform a withdraw from the backstop module
pub fn execute_withdraw(
    e: &Env,
    from: Address,
    pool_address: BytesN<32>,
    amount: i128,
) -> Result<i128, BackstopError> {
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    user.try_withdraw_shares(e, amount)?;

    let to_return = pool.convert_to_tokens(e, amount);

    // "burn" shares
    pool.withdraw(e, to_return, amount)?;
    pool.write_shares(&e);
    pool.write_tokens(&e);
    pool.write_q4w(&e);

    user.write_q4w(&e);
    user.write_shares(&e);

    let backstop_client = TokenClient::new(e, &BytesN::from_array(e, &BACKSTOP_TOKEN));
    backstop_client.xfer(&e.current_contract_address(), &from, &to_return);

    Ok(to_return)
}

/********** Emissions **********/

/// Perform a claim by a pool from the backstop module
pub fn execute_claim(
    e: &Env,
    pool_address: BytesN<32>,
    to: Address,
    amount: i128,
) -> Result<(), BackstopError> {
    let mut pool = Pool::new(e, pool_address);
    pool.verify_pool(&e)?;
    pool.claim(e, amount)?;
    pool.write_emissions(&e);

    let backstop_token = TokenClient::new(e, &BytesN::from_array(e, &BACKSTOP_TOKEN));
    backstop_token.xfer(&e.current_contract_address(), &to, &amount);

    Ok(())
}

/********** Fund Management *********/

/// Perform a draw from a pool's backstop
pub fn execute_draw(
    e: &Env,
    pool_address: BytesN<32>,
    amount: i128,
    to: Address,
) -> Result<(), BackstopError> {
    let mut pool = Pool::new(e, pool_address); // TODO: Fix
    pool.verify_pool(&e)?;

    pool.withdraw(e, amount, 0)?;
    pool.write_tokens(&e);

    let backstop_token = TokenClient::new(e, &BytesN::from_array(e, &BACKSTOP_TOKEN));
    backstop_token.xfer(&e.current_contract_address(), &to, &amount);

    Ok(())
}

/// Perform a donation to a pool's backstop
pub fn execute_donate(
    e: &Env,
    from: Address,
    pool_address: BytesN<32>,
    amount: i128,
) -> Result<(), BackstopError> {
    let mut pool = Pool::new(e, pool_address);

    let backstop_token = TokenClient::new(e, &BytesN::from_array(e, &BACKSTOP_TOKEN));
    backstop_token.xfer(&from, &e.current_contract_address(), &amount);

    pool.deposit(e, amount, 0);
    pool.write_tokens(&e);

    Ok(())
}
