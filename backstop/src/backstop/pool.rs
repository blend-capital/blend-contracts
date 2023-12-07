use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{contracttype, panic_with_error, unwrap::UnwrapOptimized, Address, Env};

use crate::{constants::SCALAR_7, dependencies::PoolFactoryClient, errors::BackstopError, storage};

/// The pool's backstop data
#[derive(Clone)]
#[contracttype]
pub struct PoolBackstopData {
    pub tokens: i128,  // the number of backstop tokens held in the pool's backstop
    pub q4w_pct: i128, // the percentage of tokens queued for withdrawal
    pub blnd: i128,    // the amount of blnd held in the pool's backstop via backstop tokens
    pub usdc: i128,    // the amount of usdc held in the pool's backstop via backstop tokens
}

pub fn load_pool_backstop_data(e: &Env, address: &Address) -> PoolBackstopData {
    let pool_balance = storage::get_pool_balance(e, address);
    let q4w_pct = pool_balance
        .q4w
        .fixed_div_ceil(pool_balance.shares, SCALAR_7)
        .unwrap_optimized();

    let (blnd_per_tkn, usdc_per_tkn) = storage::get_lp_token_val(e);
    let blnd = pool_balance
        .tokens
        .fixed_mul_floor(blnd_per_tkn, SCALAR_7)
        .unwrap_optimized();
    let usdc = pool_balance
        .tokens
        .fixed_mul_floor(usdc_per_tkn, SCALAR_7)
        .unwrap_optimized();

    PoolBackstopData {
        tokens: pool_balance.tokens,
        q4w_pct,
        blnd,
        usdc,
    }
}

/// Verify the pool address was deployed by the Pool Factory.
///
/// If the pool has an outstanding balance, it is assumed that it was verified before.
///
/// ### Arguments
/// * `address` - The pool address to verify
/// * `balance` - The balance of the pool. A balance of 0 indicates the pool has not been initialized.
///
/// ### Panics
/// If the pool address cannot be verified
pub fn require_is_from_pool_factory(e: &Env, address: &Address, balance: i128) {
    if balance == 0 {
        let pool_factory_client = PoolFactoryClient::new(e, &storage::get_pool_factory(e));
        if !pool_factory_client.is_pool(address) {
            panic_with_error!(e, BackstopError::NotPool);
        }
    }
}

/// TODO: Duplicated from pool/pool/status.rs. Can this be moved to a common location?
///
/// Calculate the threshold for the pool's backstop balance
///
/// Returns true if the pool's backstop balance is above the threshold
/// NOTE: The calculation is the percentage^5 to simplify the calculation of the pools product constant.
///       Some useful calculation results:
///         - greater than 1 = 100+%
///         - 1_0000000 = 100%
///         - 0_0000100 = ~10%
///         - 0_0000003 = ~5%
///         - 0_0000000 = ~0-4%
pub fn require_pool_above_threshold(pool_backstop_data: &PoolBackstopData) -> bool {
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
    saturating_pool_pc / threshold_pc >= 1_0000000
}

/// The pool's backstop balances
#[derive(Clone)]
#[contracttype]
pub struct PoolBalance {
    pub shares: i128, // the amount of shares the pool has issued
    pub tokens: i128, // the number of tokens the pool holds in the backstop
    pub q4w: i128,    // the number of shares queued for withdrawal
}

impl PoolBalance {
    /// Convert a token balance to a share balance based on the current pool state
    ///
    /// ### Arguments
    /// * `tokens` - the token balance to convert
    pub fn convert_to_shares(&self, tokens: i128) -> i128 {
        if self.shares == 0 {
            return tokens;
        }

        tokens
            .fixed_mul_floor(self.shares, self.tokens)
            .unwrap_optimized()
    }

    /// Convert a pool share balance to a token balance based on the current pool state
    ///
    /// ### Arguments
    /// * `shares` - the pool share balance to convert
    pub fn convert_to_tokens(&self, shares: i128) -> i128 {
        if self.shares == 0 {
            return shares;
        }

        shares
            .fixed_mul_floor(self.tokens, self.shares)
            .unwrap_optimized()
    }

    /// Determine the amount of effective tokens (not queued for withdrawal) in the pool
    pub fn non_queued_tokens(&self) -> i128 {
        self.tokens - self.convert_to_tokens(self.q4w)
    }

    /// Deposit tokens and shares into the pool
    ///
    /// If this is the first time
    ///
    /// ### Arguments
    /// * `tokens` - The amount of tokens to add
    /// * `shares` - The amount of shares to add
    pub fn deposit(&mut self, tokens: i128, shares: i128) {
        self.tokens += tokens;
        self.shares += shares;
    }

    /// Withdraw tokens and shares from the pool
    ///
    /// ### Arguments
    /// * `tokens` - The amount of tokens to withdraw
    /// * `shares` - The amount of shares to withdraw
    pub fn withdraw(&mut self, e: &Env, tokens: i128, shares: i128) {
        if tokens > self.tokens || shares > self.shares || shares > self.q4w {
            panic_with_error!(e, BackstopError::InsufficientFunds);
        }
        self.tokens -= tokens;
        self.shares -= shares;
        self.q4w -= shares;
    }

    /// Queue withdraw for the pool
    ///
    /// ### Arguments
    /// * `shares` - The amount of shares to queue for withdraw
    pub fn queue_for_withdraw(&mut self, shares: i128) {
        self.q4w += shares;
    }

    /// Dequeue queued for withdraw for the pool
    ///
    /// ### Arguments
    /// * `shares` - The amount of shares to dequeue from q4w
    pub fn dequeue_q4w(&mut self, e: &Env, shares: i128) {
        if shares > self.q4w {
            panic_with_error!(e, BackstopError::InsufficientFunds);
        }
        self.q4w -= shares;
    }
}

#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::Address as _;

    use crate::testutils::{create_backstop, create_mock_pool_factory};

    use super::*;

    #[test]
    fn test_load_pool_data() {
        let e = Env::default();

        let backstop_address = create_backstop(&e);
        let pool = Address::generate(&e);

        e.as_contract(&backstop_address, || {
            storage::set_pool_balance(
                &e,
                &pool,
                &PoolBalance {
                    shares: 150_0000000,
                    tokens: 250_0000000,
                    q4w: 50_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_0500000));

            let pool_data = load_pool_backstop_data(&e, &pool);

            assert_eq!(pool_data.tokens, 250_0000000);
            assert_eq!(pool_data.q4w_pct, 0_3333334); // rounds up
            assert_eq!(pool_data.blnd, 1_250_0000000);
            assert_eq!(pool_data.usdc, 12_5000000);
        });
    }

    /********** require_is_from_pool_factory **********/

    #[test]
    fn test_require_is_from_pool_factory() {
        let e = Env::default();

        let backstop_address = create_backstop(&e);
        let pool_address = Address::generate(&e);

        let (_, mock_pool_factory) = create_mock_pool_factory(&e, &backstop_address);
        mock_pool_factory.set_pool(&pool_address);

        e.as_contract(&backstop_address, || {
            require_is_from_pool_factory(&e, &pool_address, 0);
            assert!(true);
        });
    }

    #[test]
    fn test_require_is_from_pool_factory_skips_if_balance() {
        let e = Env::default();

        let backstop_address = create_backstop(&e);
        let pool_address = Address::generate(&e);

        // don't initialize factory to force failure if pool_address is checked

        e.as_contract(&backstop_address, || {
            require_is_from_pool_factory(&e, &pool_address, 1);
            assert!(true);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #10)")]
    fn test_require_is_from_pool_factory_not_valid() {
        let e = Env::default();

        let backstop_address = create_backstop(&e);
        let pool_address = Address::generate(&e);
        let not_pool_address = Address::generate(&e);

        let (_, mock_pool_factory) = create_mock_pool_factory(&e, &backstop_address);
        mock_pool_factory.set_pool(&pool_address);

        e.as_contract(&backstop_address, || {
            require_is_from_pool_factory(&e, &not_pool_address, 0);
            assert!(false);
        });
    }

    /********** require_pool_above_threshold **********/

    #[test]
    fn test_require_pool_above_threshold_under() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 300_000_0000000,
            q4w_pct: 0,
            tokens: 20_000_0000000,
            usdc: 25_000_0000000,
        }; // ~91.2% threshold

        let result = require_pool_above_threshold(&pool_backstop_data);
        assert!(!result);
    }

    #[test]
    fn test_require_pool_above_threshold_zero() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 5_000_0000000,
            q4w_pct: 0,
            tokens: 500_0000000,
            usdc: 1_000_0000000,
        }; // ~3.6% threshold - rounds to zero in calc

        let result = require_pool_above_threshold(&pool_backstop_data);
        assert!(!result);
    }

    #[test]
    fn test_require_pool_above_threshold_over() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 364_643_0000000,
            q4w_pct: 0,
            tokens: 15_000_0000000,
            usdc: 18_100_0000000,
        }; // 100% threshold

        let result = require_pool_above_threshold(&pool_backstop_data);
        assert!(result);
    }

    #[test]
    fn test_require_pool_above_threshold_saturates() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_backstop_data = PoolBackstopData {
            blnd: 50_000_000_0000000,
            q4w_pct: 0,
            tokens: 999_999_0000000,
            usdc: 10_000_000_0000000,
        }; // 181x threshold

        let result = require_pool_above_threshold(&pool_backstop_data);
        assert!(result);
    }

    /********** Logic **********/

    #[test]
    fn test_convert_to_shares_no_shares() {
        let pool_balance = PoolBalance {
            shares: 0,
            tokens: 0,
            q4w: 0,
        };

        let to_convert = 1234567;
        let shares = pool_balance.convert_to_shares(to_convert);
        assert_eq!(shares, to_convert);
    }

    #[test]
    fn test_convert_to_shares() {
        let pool_balance = PoolBalance {
            shares: 80321,
            tokens: 103302,
            q4w: 0,
        };

        let to_convert = 1234567;
        let shares = pool_balance.convert_to_shares(to_convert);
        assert_eq!(shares, 959920);
    }

    #[test]
    fn test_convert_to_tokens_no_shares() {
        let pool_balance = PoolBalance {
            shares: 0,
            tokens: 0,
            q4w: 0,
        };

        let to_convert = 1234567;
        let shares = pool_balance.convert_to_tokens(to_convert);
        assert_eq!(shares, to_convert);
    }

    #[test]
    fn test_convert_to_tokens() {
        let pool_balance = PoolBalance {
            shares: 80321,
            tokens: 103302,
            q4w: 0,
        };

        let to_convert = 40000;
        let shares = pool_balance.convert_to_tokens(to_convert);
        assert_eq!(shares, 51444);
    }

    #[test]
    fn test_deposit() {
        let mut pool_balance = PoolBalance {
            shares: 100,
            tokens: 200,
            q4w: 25,
        };

        pool_balance.deposit(50, 25);

        assert_eq!(pool_balance.shares, 125);
        assert_eq!(pool_balance.tokens, 250);
        assert_eq!(pool_balance.q4w, 25);
    }

    #[test]
    fn test_withdraw() {
        let e = Env::default();
        let mut pool_balance = PoolBalance {
            shares: 100,
            tokens: 200,
            q4w: 25,
        };

        pool_balance.withdraw(&e, 50, 25);

        assert_eq!(pool_balance.shares, 75);
        assert_eq!(pool_balance.tokens, 150);
        assert_eq!(pool_balance.q4w, 0);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_withdraw_too_much() {
        let e = Env::default();
        let mut pool_balance = PoolBalance {
            shares: 100,
            tokens: 200,
            q4w: 25,
        };

        pool_balance.withdraw(&e, 201, 25);
    }

    #[test]
    fn test_dequeue_q4w() {
        let e = Env::default();
        let mut pool_balance = PoolBalance {
            shares: 100,
            tokens: 200,
            q4w: 25,
        };

        pool_balance.dequeue_q4w(&e, 25);

        assert_eq!(pool_balance.shares, 100);
        assert_eq!(pool_balance.tokens, 200);
        assert_eq!(pool_balance.q4w, 0);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_dequeue_q4w_too_much() {
        let e = Env::default();
        let mut pool_balance = PoolBalance {
            shares: 100,
            tokens: 200,
            q4w: 25,
        };

        pool_balance.dequeue_q4w(&e, 26);
    }

    #[test]
    fn test_q4w() {
        let e = Env::default();
        let mut pool_balance = PoolBalance {
            shares: 100,
            tokens: 200,
            q4w: 25,
        };

        pool_balance.withdraw(&e, 50, 25);

        assert_eq!(pool_balance.shares, 75);
        assert_eq!(pool_balance.tokens, 150);
        assert_eq!(pool_balance.q4w, 0);
    }
}
