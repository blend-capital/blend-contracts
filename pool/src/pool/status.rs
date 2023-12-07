use crate::{
    constants::SCALAR_7,
    dependencies::{BackstopClient, PoolBackstopData},
    storage,
};
use soroban_sdk::{Env, Symbol};

/// Update the pool status based on the backstop module
#[allow(clippy::zero_prefixed_literal)]
#[allow(clippy::inconsistent_digit_grouping)]
pub fn execute_update_pool_status(e: &Env, pool_status: u32) -> u32 {
    let mut pool_config = storage::get_pool_config(e);

    // check the pool has met minimum backstop deposits
    let backstop_id = storage::get_backstop(e);
    let backstop_client = BackstopClient::new(e, &backstop_id);

    let pool_backstop_data = backstop_client.pool_data(&e.current_contract_address());
    let threshold = calc_pool_backstop_threshold(&pool_backstop_data);
    let mut met_threshold = true;
    if threshold < SCALAR_7 {
        met_threshold = false;
    }
    // we only care about admin status' if the pool has met the threshold - otherwise default takes over
    if (pool_config.status % 2 == 0 || pool_status % 2 == 0) && met_threshold {
        let admin = storage::get_admin(&e);
        // admin auth required to set or change admin status'
        admin.require_auth();
        pool_config.status = pool_status;
        e.events()
            .publish((Symbol::new(&e, "set_status"), admin), pool_status);
    } else {
        if pool_backstop_data.q4w_pct >= 0_5000000 {
            pool_config.status = 5;
        } else if pool_backstop_data.q4w_pct >= 0_2500000 || !met_threshold {
            pool_config.status = 3;
        } else {
            pool_config.status = 1;
        }
    }
    storage::set_pool_config(e, &pool_config);

    pool_config.status
}

/// Calculate the threshold for the pool's backstop balance
///
/// Returns the threshold as a percentage^5 in SCALAR_7 points such that SCALAR_7 = 100%
/// NOTE: The result is the percentage^5 to simplify the calculation of the pools product constant.
///       Some useful results:
///         - greater than 1 = 100+%
///         - 1_0000000 = 100%
///         - 0_0000100 = ~10%
///         - 0_0000003 = ~5%
///         - 0_0000000 = ~0-4%
pub fn calc_pool_backstop_threshold(pool_backstop_data: &PoolBackstopData) -> i128 {
    // @dev: Calculation for pools product constant of underlying will often overflow i128
    //       so saturating mul is used. This is safe because the threshold is below i128::MAX and the
    //       protocol does not need to differentiate between pools over the threshold product constant.
    //       The calculation is:
    //        - Threshold % = (bal_blnd^4 * bal_usdc) / PC^5 such that PC is 200k
    let threshold_pc = 320_000_000_000_000_000_000_000_000i128; // 3.2e26 (200k^5)
                                                                // floor balances to nearest full unit and calculate saturated pool product constant
                                                                // and scale to SCALAR_7 to get final division result in SCALAR_7 points
    let bal_blnd = pool_backstop_data.blnd / SCALAR_7;
    let bal_usdc = pool_backstop_data.usdc / SCALAR_7;
    let saturating_pool_pc = bal_blnd
        .saturating_mul(bal_blnd)
        .saturating_mul(bal_blnd)
        .saturating_mul(bal_blnd)
        .saturating_mul(bal_usdc)
        .saturating_mul(SCALAR_7); // 10^7 * 10^7
    saturating_pool_pc / threshold_pc
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::PoolConfig,
        testutils::{
            create_backstop, create_comet_lp_pool, create_pool, create_token_contract,
            setup_backstop,
        },
    };

    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Address};

    #[test]
    fn test_set_pool_status() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::generate(&e);

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &50_000_0000000);
        backstop_client.update_tkn_val();

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            execute_update_pool_status(&e, 0);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_set_pool_status_blocks_without_backstop_minimum() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::generate(&e);

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens - under limit
        blnd_client.mint(&samwise, &400_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &10_001_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &40_000_0000000,
            &vec![&e, 400_001_0000000, 10_001_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &40_000_0000000);
        backstop_client.update_tkn_val();

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            set_pool_status(&e, 0);
        });
    }

    #[test]
    fn test_update_pool_status_active() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::generate(&e);

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &50_000_0000000);
        backstop_client.update_tkn_val();

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            let status = execute_update_pool_status(&e, 11);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 1);
        });
    }

    #[test]
    fn test_update_pool_status_on_ice_tokens() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::generate(&e);

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens - under limit
        blnd_client.mint(&samwise, &400_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &10_001_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &40_000_0000000,
            &vec![&e, 400_001_0000000, 10_001_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &40_000_0000000);
        backstop_client.update_tkn_val();

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            let status = execute_update_pool_status(&e, 11);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 3);
        });
    }

    #[test]
    fn test_update_pool_status_on_ice_q4w() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::generate(&e);

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &50_000_0000000);
        backstop_client.update_tkn_val();
        backstop_client.queue_withdrawal(&samwise, &pool_id, &12_500_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            let status = execute_update_pool_status(&e, 11);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 3);
        });
    }

    #[test]
    fn test_update_pool_status_frozen() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::generate(&e);

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &50_000_0000000);
        backstop_client.update_tkn_val();
        backstop_client.queue_withdrawal(&samwise, &pool_id, &25_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            let status = execute_update_pool_status(&e, 11);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 5);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Auth, InvalidAction)")]
    fn test_update_pool_status_admin_frozen() {
        let e = Env::default();
        e.budget().reset_unlimited();
        // e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &50_000_0000000);
        backstop_client.update_tkn_val();

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 4,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            execute_update_pool_status(&e, 11);
        });
    }

    #[test]
    fn test_admin_update_pool_status_unfreeze() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        let pool_id = create_pool(&e);
        let oracle_id = Address::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &50_000_0000000);
        backstop_client.update_tkn_val();
        backstop_client.queue_withdrawal(&samwise, &pool_id, &25_000_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 5,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            let status = execute_update_pool_status(&e, 0);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 0);
        });
    }

    #[test]
    fn test_admin_update_pool_status_freeze() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();
        let pool_id = create_pool(&e);
        let oracle_id = Address::generate(&e);

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let (blnd, blnd_client) = create_token_contract(&e, &bombadil);
        let (usdc, usdc_client) = create_token_contract(&e, &bombadil);
        let (lp_token, lp_token_client) = create_comet_lp_pool(&e, &bombadil, &blnd, &usdc);
        let (backstop_id, backstop_client) = create_backstop(&e);
        setup_backstop(&e, &pool_id, &backstop_id, &lp_token, &usdc, &blnd);

        // mint lp tokens
        blnd_client.mint(&samwise, &500_001_0000000);
        blnd_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        usdc_client.mint(&samwise, &12_501_0000000);
        usdc_client.approve(&samwise, &lp_token, &i128::MAX, &99999);
        lp_token_client.join_pool(
            &50_000_0000000,
            &vec![&e, 500_001_0000000, 12_501_0000000],
            &samwise,
        );
        backstop_client.deposit(&samwise, &pool_id, &50_000_0000000);
        backstop_client.update_tkn_val();

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_admin(&e, &bombadil);
            storage::set_pool_config(&e, &pool_config);

            let status = execute_update_pool_status(&e, 4);

            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.status, status);
            assert_eq!(status, 4);
        });
    }

    #[test]
    fn test_calc_pool_backstop_threshold() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 300_000_0000000,
            q4w_pct: 0,
            tokens: 20_000_0000000,
            usdc: 25_000_0000000,
        }; // ~91.2% threshold

        let result = calc_pool_backstop_threshold(&pool_backstop_data);
        assert_eq!(result, 0_6328125);
    }

    #[test]
    fn test_calc_pool_backstop_threshold_10_percent() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 30_000_0000000,
            q4w_pct: 0,
            tokens: 1_000_0000000,
            usdc: 3_975_0000000,
        }; // ~10% threshold

        let result = calc_pool_backstop_threshold(&pool_backstop_data);
        assert_eq!(result, 0_0000100);
    }

    #[test]
    fn test_calc_pool_backstop_threshold_too_small() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 5_000_0000000,
            q4w_pct: 0,
            tokens: 500_0000000,
            usdc: 1_000_0000000,
        }; // ~3.6% threshold

        let result = calc_pool_backstop_threshold(&pool_backstop_data);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calc_pool_backstop_threshold_over() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 364_643_0000000,
            q4w_pct: 0,
            tokens: 15_000_0000000,
            usdc: 18_100_0000000,
        }; // 100% threshold

        let result = calc_pool_backstop_threshold(&pool_backstop_data);
        assert_eq!(result, 1_0000002);
    }

    #[test]
    fn test_calc_pool_backstop_threshold_saturates() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 50_000_000_0000000,
            q4w_pct: 0,
            tokens: 999_999_0000000,
            usdc: 10_000_000_0000000,
        }; // 181x threshold

        let result = calc_pool_backstop_threshold(&pool_backstop_data);
        assert_eq!(result, 53169_1198313);
    }

    #[test]
    fn test_calc_pool_backstop_threshold_10pct() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 20_000_0000000,
            q4w_pct: 0,
            tokens: 999_999_0000000,
            usdc: 20_000_0000000,
        }; // 10% threshold

        let result = calc_pool_backstop_threshold(&pool_backstop_data);
        assert_eq!(result, 0_0000100);
    }

    #[test]
    fn test_calc_pool_backstop_threshold_5pct() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 10_000_0000000,
            q4w_pct: 0,
            tokens: 999_999_0000000,
            usdc: 10_000_0000000,
        }; // 5% threshold

        let result = calc_pool_backstop_threshold(&pool_backstop_data);
        assert_eq!(result, 0_0000003);
    }
}
