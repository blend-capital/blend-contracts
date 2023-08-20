use crate::{
    constants::SCALAR_7,
    dependencies::BackstopClient,
    errors::PoolError,
    storage::{self, get_backstop_pool},
};
use fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, Env};

/// Update the pool status based on the backstop module
#[allow(clippy::zero_prefixed_literal)]
#[allow(clippy::inconsistent_digit_grouping)]
pub fn execute_update_pool_status(e: &Env) -> u32 {
    let mut pool_config = storage::get_pool_config(e);
    if pool_config.status > 2 {
        // pool has been admin frozen and can only be restored by the admin
        panic_with_error!(e, PoolError::InvalidPoolStatus);
    }

    let backstop_id = storage::get_backstop(e);
    let backstop_client = BackstopClient::new(e, &backstop_id);

    let pool_balance = backstop_client.pool_balance(&get_backstop_pool(e));
    let q4w_pct = pool_balance
        .q4w
        .fixed_div_floor(pool_balance.shares, SCALAR_7)
        .unwrap_optimized();

    if q4w_pct >= 0_5000000 {
        pool_config.status = 2;
        //TODO: this token check needs to check for k-value of over 200,000 for pool balance LP tokens
    } else if q4w_pct >= 0_2500000 || pool_balance.tokens < 1_000_000_0000000 {
        pool_config.status = 1;
    } else {
        pool_config.status = 0;
    }
    storage::set_pool_config(e, &pool_config);

    pool_config.status
}

/// Update the pool status
#[allow(clippy::inconsistent_digit_grouping)]
pub fn set_pool_status(e: &Env, pool_status: u32) {
    if pool_status == 0 {
        // check the pool has met minimum backstop deposits before being turned on
        let backstop_id = storage::get_backstop(e);
        let backstop_client = BackstopClient::new(e, &backstop_id);

        let pool_balance = backstop_client.pool_balance(&e.current_contract_address());
        if pool_balance.tokens < 200_000_000_0000 {
            panic_with_error!(e, PoolError::InvalidPoolStatus);
        }
    }

    let mut pool_config = storage::get_pool_config(e);
    pool_config.status = pool_status;
    storage::set_pool_config(e, &pool_config);
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::PoolConfig,
        testutils::{create_backstop, create_token_contract, setup_backstop},
    };

    use super::*;
    use soroban_sdk::{testutils::Address as _, Address};

    #[test]
    fn test_set_pool_status() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();
        let pool_id = Address::random(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &Address::random(&e),
        );
        backstop_token_client.mint(&samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop_pool(&e, &pool_id);

            set_pool_status(&e, 0);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, 0);
        });
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(11))")]
    fn test_set_pool_status_blocks_without_backstop_minimum() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();
        let pool_id = Address::random(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &Address::random(&e),
        );
        backstop_token_client.mint(&samwise, &199_999_9999999);
        backstop_client.deposit(&samwise, &pool_id, &199_999_9999999);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop_pool(&e, &pool_id);

            set_pool_status(&e, 0);
        });
    }

    #[test]
    fn test_update_pool_status_active() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();
        let pool_id = Address::random(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &Address::random(&e),
        );
        backstop_token_client.mint(&samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop_pool(&e, &pool_id);

            let status = execute_update_pool_status(&e);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 0);
        });
    }

    #[test]
    fn test_update_pool_status_on_ice_tokens() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();
        let pool_id = Address::random(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &Address::random(&e),
        );
        backstop_token_client.mint(&samwise, &900_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &900_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop_pool(&e, &pool_id);

            let status = execute_update_pool_status(&e);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 1);
        });
    }

    #[test]
    fn test_update_pool_status_on_ice_q4w() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();
        let pool_id = Address::random(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &Address::random(&e),
        );
        backstop_token_client.mint(&samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);
        backstop_client.queue_withdrawal(&samwise, &pool_id, &300_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop_pool(&e, &pool_id);

            let status = execute_update_pool_status(&e);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 1);
        });
    }

    #[test]
    fn test_update_pool_status_frozen() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();
        let pool_id = Address::random(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &Address::random(&e),
        );
        backstop_token_client.mint(&samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);
        backstop_client.queue_withdrawal(&samwise, &pool_id, &600_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop_pool(&e, &pool_id);

            let status = execute_update_pool_status(&e);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 2);
        });
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(11))")]
    fn test_update_pool_status_admin_frozen() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();
        let pool_id = Address::random(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &Address::random(&e),
        );
        backstop_token_client.mint(&samwise, &1_100_000_0000000);
        backstop_client.deposit(&samwise, &pool_id, &1_100_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 3,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop_pool(&e, &pool_id);

            execute_update_pool_status(&e);
        });
    }
}
