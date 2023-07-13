use fixed_point_math::FixedPoint;
use soroban_sdk::{contracttype, panic_with_error, unwrap::UnwrapOptimized, Address, Env};

use crate::{dependencies::PoolFactoryClient, errors::BackstopError, storage};

/// Verify the pool address was deployed by the Pool Factory
///
/// Panics if the pool address cannot be verified
pub fn require_is_from_pool_factory(e: &Env, address: &Address) {
    let pool_factory_client = PoolFactoryClient::new(e, &storage::get_pool_factory(e));
    if !pool_factory_client.is_pool(address) {
        panic_with_error!(e, BackstopError::NotPool);
    }
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
    pub fn default() -> PoolBalance {
        PoolBalance {
            shares: 0,
            tokens: 0,
            q4w: 0,
        }
    }

    /// Convert a token balance to a share balance based on the current pool state
    ///
    /// ### Arguments
    /// * `tokens` - the token balance to convert
    pub fn convert_to_shares(&mut self, tokens: i128) -> i128 {
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
    pub fn convert_to_tokens(&mut self, shares: i128) -> i128 {
        if self.shares == 0 {
            return shares;
        }

        shares
            .fixed_mul_floor(self.tokens, self.shares)
            .unwrap_optimized()
    }

    /// Deposit tokens and shares into the pool
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

    use crate::testutils::create_mock_pool_factory;

    use super::*;

    /********** require_is_from_pool_factory **********/

    #[test]
    fn test_require_is_from_pool_factory() {
        let e = Env::default();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);

        let (_, mock_pool_factory) = create_mock_pool_factory(&e, &backstop_address);
        mock_pool_factory.set_pool(&pool_address);

        e.as_contract(&backstop_address, || {
            require_is_from_pool_factory(&e, &pool_address);
            assert!(true);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(10)")]
    fn test_require_is_from_pool_factory_not_valid() {
        let e = Env::default();

        let backstop_address = Address::random(&e);
        let pool_address = Address::random(&e);
        let not_pool_address = Address::random(&e);

        let (_, mock_pool_factory) = create_mock_pool_factory(&e, &backstop_address);
        mock_pool_factory.set_pool(&pool_address);

        e.as_contract(&backstop_address, || {
            require_is_from_pool_factory(&e, &not_pool_address);
            assert!(false);
        });
    }

    /********** Logic **********/

    #[test]
    fn test_convert_to_shares_no_shares() {
        let mut pool_balance = PoolBalance {
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
        let mut pool_balance = PoolBalance {
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
        let mut pool_balance = PoolBalance {
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
        let mut pool_balance = PoolBalance {
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
    #[should_panic(expected = "ContractError(6)")]
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
    #[should_panic(expected = "ContractError(6)")]
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
