use crate::{constants::SCALAR_7, dependencies::BackstopClient, errors::PoolError, storage};
use fixed_point_math::FixedPoint;
use soroban_sdk::{Address, Env};

/// Update the pool status based on the backstop module
pub fn execute_update_pool_status(e: &Env) -> Result<u32, PoolError> {
    let mut pool_config = storage::get_pool_config(e);
    if pool_config.status > 2 {
        // pool has been admin frozen and can only be restored by the admin
        return Err(PoolError::InvalidPoolStatus);
    }

    let backstop_id = storage::get_backstop(e);
    let backstop_client = BackstopClient::new(e, &backstop_id);

    let (pool_tokens, pool_shares, shares_q4w) =
        backstop_client.p_balance(&e.current_contract_id());
    let q4w_pct = shares_q4w.fixed_div_floor(pool_shares, SCALAR_7).unwrap();

    if q4w_pct >= 0_5000000 {
        pool_config.status = 2;
    } else if q4w_pct >= 0_2500000 || pool_tokens < 1_000_000_0000000 {
        pool_config.status = 1;
    } else {
        pool_config.status = 0;
    }
    storage::set_pool_config(e, &pool_config);

    Ok(pool_config.status)
}

/// Update the pool status
pub fn set_pool_status(e: &Env, admin: &Address, pool_status: u32) -> Result<(), PoolError> {
    if admin.clone() != storage::get_admin(e) {
        return Err(PoolError::NotAuthorized);
    }

    if pool_status == 0 {
        // check the pool has met minimum backstop deposits before being turned on
        let backstop_id = storage::get_backstop(e);
        let backstop_client = BackstopClient::new(e, &backstop_id);

        let (pool_tokens, _, _) = backstop_client.p_balance(&e.current_contract_id());
        if pool_tokens < 1_000_000_0000000 {
            return Err(PoolError::InvalidPoolStatus);
        }
    }

    let mut pool_config = storage::get_pool_config(e);
    pool_config.status = pool_status;
    storage::set_pool_config(e, &pool_config);

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::PoolConfig,
        testutils::{create_backstop, create_backstop_token},
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        BytesN,
    };

    #[test]
    fn test_set_pool_status() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            set_pool_status(&e, &bombadil, 0).unwrap();

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, 0);
        });
    }

    #[test]
    fn test_set_pool_status_requires_admin() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let sauron = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            let result = set_pool_status(&e, &sauron, 0);
            assert_eq!(result, Err(PoolError::NotAuthorized));

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, 1);
        });
    }

    #[test]
    fn test_set_pool_status_blocks_without_backstop_minimum() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &999_999_9999999);
        backstop_client.deposit(&samwise, &pool_id, &999_999_9999999);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            let result = set_pool_status(&e, &bombadil, 0);
            assert_eq!(result, Err(PoolError::InvalidPoolStatus));
        });
    }

    #[test]
    fn test_update_pool_status_active() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            let status = execute_update_pool_status(&e).unwrap();

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 0);
        });
    }

    #[test]
    fn test_update_pool_status_on_ice_tokens() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &900_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &900_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            let status = execute_update_pool_status(&e).unwrap();

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 1);
        });
    }

    #[test]
    fn test_update_pool_status_on_ice_q4w() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);
        backstop_client.q_withdraw(&samwise, &pool_id, &300_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            let status = execute_update_pool_status(&e).unwrap();

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 1);
        });
    }

    #[test]
    fn test_update_pool_status_frozen() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);
        backstop_client.q_withdraw(&samwise, &pool_id, &600_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            let status = execute_update_pool_status(&e).unwrap();

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 2);
        });
    }

    #[test]
    fn test_update_pool_status_admin_frozen() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let oracle_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_id, backstop_client) = create_backstop(&e);
        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 3,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            let result = execute_update_pool_status(&e);
            assert_eq!(result, Err(PoolError::InvalidPoolStatus));
        });
    }
}
