use crate::{
    contract::require_nonnegative, dependencies::TokenClient, emissions, errors::BackstopError,
    pool::Pool, storage, user::User,
};
use soroban_sdk::{Address, Env};

/// Perform a queue for withdraw from the backstop module
pub fn execute_queue_withdrawal(
    e: &Env,
    from: &Address,
    pool_address: &Address,
    amount: i128,
) -> Result<storage::Q4W, BackstopError> {
    require_nonnegative(amount)?;
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    let new_q4w = user.try_queue_shares_for_withdrawal(e, amount)?;
    user.write_q4w(&e);

    pool.queue_for_withdraw(e, amount);
    pool.write_q4w(&e);

    Ok(new_q4w)
}

/// Perform a dequeue of queued for withdraw deposits from the backstop module
pub fn execute_dequeue_withdrawal(
    e: &Env,
    from: &Address,
    pool_address: &Address,
    amount: i128,
) -> Result<(), BackstopError> {
    require_nonnegative(amount)?;
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
    pool_address: &Address,
    amount: i128,
) -> Result<i128, BackstopError> {
    require_nonnegative(amount)?;
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    emissions::update_emission_index(e, &mut pool, &mut user, false)?;

    user.try_withdraw_shares(e, amount)?;

    let to_return = pool.convert_to_tokens(e, amount);

    // "burn" shares
    pool.withdraw(e, to_return, amount)?;
    pool.write_shares(&e);
    pool.write_tokens(&e);
    pool.write_q4w(&e);

    user.write_q4w(&e);
    user.write_shares(&e);

    let backstop_token_client = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token_client.transfer(&e.current_contract_address(), &from, &to_return);

    Ok(to_return)
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, Address,
    };

    use crate::{
        backstop::{execute_deposit, execute_donate},
        storage::Q4W,
        testutils::{assert_eq_vec_q4w, create_backstop_token},
    };

    use super::*;

    #[test]
    fn test_execute_queue_withdrawal() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);

        // setup pool with deposits
        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &samwise, &pool_address, 100_0000000).unwrap();
        });

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 10000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_address, || {
            execute_queue_withdrawal(&e, &samwise, &pool_address, 42_0000000).unwrap();
            assert_eq!(
                storage::get_shares(&e, &pool_address, &samwise),
                100_0000000
            );
            let q4w = storage::get_q4w(&e, &pool_address, &samwise);
            let expected_q4w = vec![
                &e,
                Q4W {
                    amount: 42_0000000,
                    exp: 10000 + 30 * 24 * 60 * 60,
                },
            ];
            assert_eq_vec_q4w(&q4w, &expected_q4w);
            assert_eq!(storage::get_pool_q4w(&e, &pool_address), 42_0000000);
            assert_eq!(storage::get_pool_shares(&e, &pool_address), 100_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_address), 100_0000000);
            assert_eq!(
                backstop_token_client.balance(&backstop_address),
                100_0000000
            );
            assert_eq!(backstop_token_client.balance(&samwise), 0);
        });
    }
    #[test]
    fn test_execute_queue_withdrawal_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);

        // setup pool with deposits
        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &samwise, &pool_address, 100_0000000).unwrap();
        });

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 10000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_address, || {
            let res = execute_queue_withdrawal(&e, &samwise, &pool_address, -42_0000000);
            match res {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::NegativeAmount => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_execute_dequeue_withdrawal() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);

        // queue shares for withdraw
        e.as_contract(&backstop_address, || {
            let total_shares = execute_deposit(&e, &samwise, &pool_address, 75_0000000).unwrap();
            assert_eq!(backstop_token_client.balance(&samwise), 25_0000000);
            assert_eq!(
                storage::get_shares(&e, &pool_address, &samwise),
                total_shares
            );
            assert_eq!(total_shares, 75_0000000);

            execute_queue_withdrawal(&e, &samwise, &pool_address, 25_0000000).unwrap();

            e.ledger().set(LedgerInfo {
                protocol_version: 1,
                sequence_number: 100,
                timestamp: 10000,
                network_id: Default::default(),
                base_reserve: 10,
            });

            execute_queue_withdrawal(&e, &samwise, &pool_address, 40_0000000).unwrap();
        });

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 20000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_address, || {
            execute_dequeue_withdrawal(&e, &samwise, &pool_address, 30_0000000).unwrap();
            assert_eq!(storage::get_shares(&e, &pool_address, &samwise), 75_0000000);
            let q4w = storage::get_q4w(&e, &pool_address, &samwise);
            let expected_q4w = vec![
                &e,
                Q4W {
                    amount: 35_0000000,
                    exp: 10000 + 30 * 24 * 60 * 60,
                },
            ];
            assert_eq_vec_q4w(&q4w, &expected_q4w);
            assert_eq!(storage::get_pool_q4w(&e, &pool_address), 35_0000000);
            assert_eq!(storage::get_pool_shares(&e, &pool_address), 75_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_address), 75_0000000);
        });
    }
    #[test]
    fn test_execute_dequeue_withdrawal_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);

        // queue shares for withdraw
        e.as_contract(&backstop_address, || {
            let total_shares = execute_deposit(&e, &samwise, &pool_address, 75_0000000).unwrap();
            assert_eq!(backstop_token_client.balance(&samwise), 25_0000000);
            assert_eq!(
                storage::get_shares(&e, &pool_address, &samwise),
                total_shares
            );
            assert_eq!(total_shares, 75_0000000);

            execute_queue_withdrawal(&e, &samwise, &pool_address, 25_0000000).unwrap();

            e.ledger().set(LedgerInfo {
                protocol_version: 1,
                sequence_number: 100,
                timestamp: 10000,
                network_id: Default::default(),
                base_reserve: 10,
            });

            execute_queue_withdrawal(&e, &samwise, &pool_address, 40_0000000).unwrap();
        });

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 20000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_address, || {
            let res = execute_dequeue_withdrawal(&e, &samwise, &pool_address, -30_0000000);
            match res {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::NegativeAmount => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_execute_withdrawal() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &150_0000000);

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 10000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        // setup pool with queue for withdrawal and allow the backstop to incur a profit
        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &samwise, &pool_address, 100_0000000).unwrap();
            execute_queue_withdrawal(&e, &samwise, &pool_address, 42_0000000).unwrap();
            execute_donate(&e, &samwise, &pool_address, 50_0000000).unwrap();
        });

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 10000 + 30 * 24 * 60 * 60 + 1,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_address, || {
            let tokens = execute_withdraw(&e, &samwise, &pool_address, 42_0000000).unwrap();
            assert_eq!(
                storage::get_shares(&e, &pool_address, &samwise),
                100_0000000 - 42_0000000
            );
            let q4w = storage::get_q4w(&e, &pool_address, &samwise);
            assert_eq!(q4w.len(), 0);
            assert_eq!(storage::get_pool_q4w(&e, &pool_address), 0);
            assert_eq!(
                storage::get_pool_shares(&e, &pool_address),
                100_0000000 - 42_0000000
            );
            assert_eq!(
                storage::get_pool_tokens(&e, &pool_address),
                150_0000000 - tokens
            );
            assert_eq!(tokens, 63_0000000);
            assert_eq!(
                backstop_token_client.balance(&backstop_address),
                150_0000000 - tokens
            );
            assert_eq!(backstop_token_client.balance(&samwise), tokens);
        });
    }
    #[test]
    fn test_execute_withdrawal_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &150_0000000);

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 10000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        // setup pool with queue for withdrawal and allow the backstop to incur a profit
        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &samwise, &pool_address, 100_0000000).unwrap();
            execute_queue_withdrawal(&e, &samwise, &pool_address, 42_0000000).unwrap();
            execute_donate(&e, &samwise, &pool_address, 50_0000000).unwrap();
        });

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 200,
            timestamp: 10000 + 30 * 24 * 60 * 60 + 1,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_address, || {
            let res = execute_withdraw(&e, &samwise, &pool_address, -42_0000000);
            match res {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::NegativeAmount => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }
}
