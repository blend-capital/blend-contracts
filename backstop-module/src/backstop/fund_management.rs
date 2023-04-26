use crate::{
    contract::require_nonnegative, dependencies::TokenClient, errors::BackstopError, pool::Pool,
    storage,
};
use soroban_sdk::{Address, BytesN, Env};

/// Perform a draw from a pool's backstop
pub fn execute_draw(
    e: &Env,
    pool_address: &BytesN<32>,
    amount: i128,
    to: &Address,
) -> Result<(), BackstopError> {
    require_nonnegative(amount)?;
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
    require_nonnegative(amount)?;
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
        testutils::{Address as _, BytesN as _},
        Address, BytesN,
    };

    use crate::{
        backstop::execute_deposit,
        testutils::{create_backstop_token, create_mock_pool_factory},
    };

    use super::*;

    #[test]
    fn test_execute_donate() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let pool_0_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &100_0000000);
        backstop_token_client.mint(&bombadil, &frodo, &100_0000000);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 25_0000000).unwrap();
        });

        e.as_contract(&backstop_id, || {
            execute_donate(&e, &samwise, &pool_0_id, 30_0000000).unwrap();
            assert_eq!(storage::get_pool_shares(&e, &pool_0_id), 25_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_0_id), 55_0000000);
        });
    }

    #[test]
    fn test_execute_donate_negative_amount() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let pool_0_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &100_0000000);
        backstop_token_client.mint(&bombadil, &frodo, &100_0000000);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 25_0000000).unwrap();
        });

        e.as_contract(&backstop_id, || {
            let res = execute_donate(&e, &samwise, &pool_0_id, -30_0000000);
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
    fn test_execute_draw() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        let pool_0_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_id);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000).unwrap();
        });

        e.as_contract(&backstop_id, || {
            execute_draw(&e, &pool_0_id, 30_0000000, &samwise).unwrap();
            assert_eq!(storage::get_pool_shares(&e, &pool_0_id), 50_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_0_id), 20_0000000);
            assert_eq!(backstop_token_client.balance(&backstop), 20_0000000);
            assert_eq!(backstop_token_client.balance(&samwise), 30_0000000);
        });
    }

    #[test]
    fn test_execute_draw_requires_pool_factory_verification() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let pool_0_id = BytesN::<32>::random(&e);
        let pool_bad_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_id);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000).unwrap();
        });

        e.as_contract(&backstop_id, || {
            let result = execute_draw(&e, &pool_bad_id, 30_0000000, &samwise);
            assert_eq!(result, Err(BackstopError::NotPool));
        });
    }

    #[test]
    fn test_execute_draw_only_can_take_from_pool() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let pool_0_id = BytesN::<32>::random(&e);
        let pool_1_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_id);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000).unwrap();
            execute_deposit(&e, &frodo, &pool_1_id, 50_0000000).unwrap();
        });

        e.as_contract(&backstop_id, || {
            let result = execute_draw(&e, &pool_0_id, 51_0000000, &samwise);
            assert_eq!(result, Err(BackstopError::InsufficientFunds));
        });
    }

    #[test]
    fn test_execute_draw_negative_amount() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let pool_0_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_id);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000).unwrap();
        });

        e.as_contract(&backstop_id, || {
            let res = execute_draw(&e, &pool_0_id, -30_0000000, &samwise);
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
