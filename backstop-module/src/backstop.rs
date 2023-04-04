use crate::{dependencies::TokenClient, errors::BackstopError, pool::Pool, storage, user::User};
use soroban_sdk::{Address, BytesN, Env};

/// Perform a deposit into the backstop module
pub fn execute_deposit(
    e: &Env,
    from: &Address,
    pool_address: &BytesN<32>,
    amount: i128,
) -> Result<i128, BackstopError> {
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    let to_mint = pool.convert_to_shares(e, amount);

    let backstop_token_client = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token_client.xfer(&from, &e.current_contract_address(), &amount);

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
    from: &Address,
    pool_address: &BytesN<32>,
    amount: i128,
) -> Result<storage::Q4W, BackstopError> {
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    let new_q4w = user.try_queue_shares_for_withdrawal(e, amount)?;
    user.write_q4w(&e);

    pool.queue_for_withdraw(e, amount);
    pool.write_q4w(&e);

    Ok(new_q4w)
}

/// Perform a dequeue of queued for withdraw deposits from the backstop module
pub fn execute_dequeue_q4w(
    e: &Env,
    from: &Address,
    pool_address: &BytesN<32>,
    amount: i128,
) -> Result<(), BackstopError> {
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    user.try_dequeue_shares_for_withdrawal(e, amount, false)?;

    // remove shares from q4w
    pool.dequeue_q4w(e, amount)?;
    pool.write_q4w(&e);

    user.write_q4w(&e);
    Ok(())
}

/// Perform a withdraw from the backstop module
pub fn execute_withdraw(
    e: &Env,
    from: &Address,
    pool_address: &BytesN<32>,
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

    let backstop_client = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_client.xfer(&e.current_contract_address(), &from, &to_return);

    Ok(to_return)
}

/********** Emissions **********/

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

/********** Fund Management *********/

/// Perform a draw from a pool's backstop
pub fn execute_draw(
    e: &Env,
    pool_address: &BytesN<32>,
    amount: i128,
    to: &Address,
) -> Result<(), BackstopError> {
    let mut pool = Pool::new(e, pool_address.clone()); // TODO: Fix
    pool.verify_pool(&e)?;

    pool.withdraw(e, amount, 0)?;
    pool.write_tokens(&e);

    let backstop_token = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token.xfer(&e.current_contract_address(), &to, &amount);

    Ok(())
}

/// Perform a donation to a pool's backstop
pub fn execute_donate(
    e: &Env,
    from: &Address,
    pool_address: &BytesN<32>,
    amount: i128,
) -> Result<(), BackstopError> {
    let mut pool = Pool::new(e, pool_address.clone());

    let backstop_token = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token.xfer(&from, &e.current_contract_address(), &amount);

    pool.deposit(e, amount, 0);
    pool.write_tokens(&e);

    Ok(())
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _, Ledger, LedgerInfo},
        vec, Address, BytesN,
    };

    use crate::{
        storage::Q4W,
        testutils::{assert_eq_vec_q4w, create_backstop_token},
    };

    use super::*;

    /********** execute_dequeue_q4w **********/

    #[test]
    fn test_execute_dequeue_q4w() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let pool_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &100_0000000);

        // queue shares for withdraw
        e.as_contract(&backstop_id, || {
            let total_shares = execute_deposit(&e, &samwise, &pool_id, 75_0000000).unwrap();
            assert_eq!(backstop_token_client.balance(&samwise), 25_0000000);
            assert_eq!(storage::get_shares(&e, &pool_id, &samwise), total_shares);
            assert_eq!(total_shares, 75_0000000);

            execute_q_withdraw(&e, &samwise, &pool_id, 25_0000000).unwrap();

            e.ledger().set(LedgerInfo {
                protocol_version: 1,
                sequence_number: 100,
                timestamp: 10000,
                network_id: Default::default(),
                base_reserve: 10,
            });

            execute_q_withdraw(&e, &samwise, &pool_id, 40_0000000).unwrap();
        });

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 20000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_id, || {
            execute_dequeue_q4w(&e, &samwise, &pool_id, 30_0000000).unwrap();
            assert_eq!(storage::get_shares(&e, &pool_id, &samwise), 75_0000000);
            let q4w = storage::get_q4w(&e, &pool_id, &samwise);
            let expected_q4w = vec![
                &e,
                Q4W {
                    amount: 35_0000000,
                    exp: 10000 + 30 * 24 * 60 * 60,
                },
            ];
            assert_eq_vec_q4w(&q4w, &expected_q4w);
            assert_eq!(storage::get_pool_q4w(&e, &pool_id), 35_0000000);
            assert_eq!(storage::get_pool_shares(&e, &pool_id), 75_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_id), 75_0000000);
        });
    }
}
