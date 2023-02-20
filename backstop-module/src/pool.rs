use fixed_point_math::FixedPoint;
use soroban_sdk::{Address, BytesN, Env};

use crate::{
    constants::POOL_FACTORY, dependencies::PoolFactoryClient, errors::BackstopError, storage,
};

/// A user of the backstop module with respect to a given pool
/// Data is lazy loaded as not all struct information is required for each action
pub struct Pool {
    pub contract_id: BytesN<32>,
    shares: Option<i128>,
    tokens: Option<i128>,
    q4w: Option<i128>,
    emissions: Option<i128>,
}

impl Pool {
    pub fn new(e: &Env, contract_id: BytesN<32>) -> Pool {
        Pool {
            contract_id: contract_id.clone(),
            shares: None,
            tokens: None,
            q4w: None,
            emissions: None,
        }
    }

    /// Verify the pool address was deployed by the Pool Factory
    ///
    /// Returns a Result
    pub fn verify_pool(&self, e: &Env) -> Result<(), BackstopError> {
        let pool_factory_client = PoolFactoryClient::new(e, &BytesN::from_array(&e, &POOL_FACTORY));
        if !pool_factory_client.is_pool(&self.contract_id) {
            return Err(BackstopError::NotPool);
        }
        Ok(())
    }

    /********** Setters / Lazy Getters / Storage **********/

    /// Get the pool's total issued shares from the cache or the ledger
    pub fn get_shares(&mut self, e: &Env) -> i128 {
        match self.shares {
            Some(bal) => bal,
            None => {
                let bal = storage::get_pool_shares(e, &self.contract_id);
                self.shares = Some(bal);
                bal
            }
        }
    }

    /// Set the pool's total issued shares to the cache
    ///
    /// ### Arguments
    /// * `shares` - The pool's total issued shares
    pub fn set_shares(&mut self, shares: i128) {
        self.shares = Some(shares)
    }

    /// Write the currently cached pool's total issued shares to the ledger
    pub fn write_shares(&self, e: &Env) {
        match self.shares {
            Some(bal) => storage::set_pool_shares(e, &self.contract_id, &bal),
            None => panic!("nothing to write"),
        }
    }

    /// Get the pool's total queued for withdraw from the cache or the ledger
    pub fn get_q4w(&mut self, e: &Env) -> i128 {
        match self.q4w.clone() {
            Some(q4w) => q4w,
            None => {
                let q4w = storage::get_pool_q4w(e, &self.contract_id);
                self.q4w = Some(q4w);
                q4w
            }
        }
    }

    /// Set the pool's total queued for withdraw to the cache
    ///
    /// ### Arguments
    /// * `q4w` - The pool's total queued for withdraw
    pub fn set_q4w(&mut self, q4w: i128) {
        self.q4w = Some(q4w)
    }

    /// Write the currently cached pool's total queued for withdraw to the ledger
    pub fn write_q4w(&self, e: &Env) {
        match self.q4w {
            Some(q4w) => storage::set_pool_q4w(e, &self.contract_id, &q4w),
            None => panic!("nothing to write"),
        }
    }

    /// Get the pool's total backstop tokens from the cache or the ledger
    pub fn get_tokens(&mut self, e: &Env) -> i128 {
        match self.tokens {
            Some(bal) => bal,
            None => {
                let bal = storage::get_pool_tokens(e, &self.contract_id);
                self.tokens = Some(bal);
                bal
            }
        }
    }

    /// Set the pool's total backstop tokens to the cache
    ///
    /// ### Arguments
    /// * `tokens` - The pool's backstop tokens
    pub fn set_tokens(&mut self, tokens: i128) {
        self.tokens = Some(tokens)
    }

    /// Write the currently cached pool's total backstop tokens to the ledger
    pub fn write_tokens(&self, e: &Env) {
        match self.tokens {
            Some(bal) => storage::set_pool_tokens(e, &self.contract_id, &bal),
            None => panic!("nothing to write"),
        }
    }

    /// Get the pool's total emissions tokens from the cache or the ledger
    pub fn get_emissions(&mut self, e: &Env) -> i128 {
        match self.emissions {
            Some(bal) => bal,
            None => {
                let bal = storage::get_pool_emis(e, &self.contract_id);
                self.emissions = Some(bal);
                bal
            }
        }
    }

    /// Set the pool's total emissions
    ///
    /// ### Arguments
    /// * `amount` - The pool's emissions
    pub fn set_emissions(&mut self, amount: i128) {
        self.emissions = Some(amount)
    }

    /// Write the currently cached pool's total emissions to the ledger
    pub fn write_emissions(&self, e: &Env) {
        match self.emissions {
            Some(bal) => storage::set_pool_emis(e, &self.contract_id, &bal),
            None => panic!("nothing to write"),
        }
    }

    /********** Logic **********/

    /// Convert a token balance to a share balance based on the current pool state
    ///
    /// ### Arguments
    /// * `tokens` - the token balance to convert
    pub fn convert_to_shares(&mut self, e: &Env, tokens: i128) -> i128 {
        let pool_shares = self.get_shares(e);
        if pool_shares == 0 {
            return tokens;
        }

        tokens
            .fixed_mul_floor(pool_shares, self.get_tokens(e))
            .unwrap()
    }

    /// Convert a pool share balance to a token balance based on the current pool state
    ///
    /// ### Arguments
    /// * `shares` - the pool share balance to convert
    pub fn convert_to_tokens(&mut self, e: &Env, shares: i128) -> i128 {
        let pool_shares = self.get_shares(e);
        if pool_shares == 0 {
            return shares;
        }

        shares
            .fixed_mul_floor(self.get_tokens(e), pool_shares)
            .unwrap()
    }

    /// Deposit tokens and shares into the pool
    ///
    /// Updates cached values but does not write:
    /// * tokens
    /// * shares
    ///
    /// ### Arguments
    /// * `tokens` - The amount of tokens to add
    /// * `shares` - The amount of shares to add
    pub fn deposit(&mut self, e: &Env, tokens: i128, shares: i128) {
        let cur_tokens = self.get_tokens(e);
        let cur_shares = self.get_shares(e);
        self.set_tokens(cur_tokens + tokens);
        self.set_shares(cur_shares + shares);
    }

    /// Withdraw tokens and shares from the pool
    ///
    /// Updates cached values but does not write:
    /// * tokens
    /// * shares
    /// * q4w
    ///
    /// ### Arguments
    /// * `tokens` - The amount of tokens to withdraw
    /// * `shares` - The amount of shares to withdraw
    pub fn withdraw(&mut self, e: &Env, tokens: i128, shares: i128) -> Result<(), BackstopError> {
        let cur_tokens = self.get_tokens(e);
        let cur_shares = self.get_shares(e);
        let cur_q4w = self.get_q4w(e);
        if tokens > cur_tokens || shares > cur_shares || shares > cur_q4w {
            return Err(BackstopError::InsufficientFunds);
        }
        self.set_tokens(cur_tokens - tokens);
        self.set_shares(cur_shares - shares);
        self.set_q4w(cur_q4w - shares);
        Ok(())
    }

    /// Queue withdraw for the pool
    ///
    /// Updates cached values but does not write:
    /// * q4w
    ///
    /// ### Arguments
    /// * `shares` - The amount of shares to queue for withdraw
    pub fn queue_for_withdraw(&mut self, e: &Env, shares: i128) {
        let cur_q4w = self.get_q4w(e);
        self.set_q4w(cur_q4w + shares);
    }

    /// Claim emissions from the pool
    ///
    /// Updates cached values but does not write:
    /// * emissions
    ///
    /// ### Arguments
    /// * `amount` - The amount of emissions you are trying to claim
    pub fn claim(&mut self, e: &Env, amount: i128) -> Result<(), BackstopError> {
        let emissions = self.get_emissions(e);
        if emissions < amount {
            return Err(BackstopError::InsufficientFunds);
        }
        self.set_emissions(emissions - amount);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::testutils::{create_mock_pool_factory, generate_contract_id};

    use super::*;

    /********** Verification **********/

    #[test]
    fn test_verify_pool_valid() {
        let e = Env::default();

        let pool_addr = generate_contract_id(&e);

        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool_addr);

        let pool = Pool::new(&e, pool_addr.clone());
        let result = pool.verify_pool(&e);

        match result {
            Ok(_) => {
                assert!(true)
            }
            Err(_) => {
                assert!(false)
            }
        }
    }

    #[test]
    fn test_verify_pool_not_valid() {
        let e = Env::default();

        let pool_addr = generate_contract_id(&e);
        let not_pool_addr = generate_contract_id(&e);

        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool_addr);

        let pool = Pool::new(&e, not_pool_addr.clone());
        let result = pool.verify_pool(&e);

        match result {
            Ok(_) => assert!(false),
            Err(err) => assert_eq!(err, BackstopError::NotPool),
        }
    }

    /********** Cache / Getters / Setters **********/

    #[test]
    fn test_share_cache() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool::new(&e, pool_addr.clone());

        let first_share_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage::set_pool_shares(&e, &pool_addr, &first_share_amt);
            let first_result = pool.get_shares(&e);
            assert_eq!(first_result, first_share_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage::set_pool_shares(&e, &pool_addr, &1);
            let cached_result = pool.get_shares(&e);
            assert_eq!(cached_result, first_share_amt);

            // new amount gets set and stored
            let second_share_amt = 200;
            pool.set_shares(second_share_amt);
            let second_result = pool.get_shares(&e);
            assert_eq!(second_result, second_share_amt);

            // write stores to chain
            pool.write_shares(&e);
            let chain_result = storage::get_pool_shares(&e, &pool_addr);
            assert_eq!(chain_result, second_share_amt);
        });
    }

    #[test]
    fn test_q4w_cache() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool::new(&e, pool_addr.clone());

        let first_q4w_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage::set_pool_q4w(&e, &pool_addr, &first_q4w_amt);
            let first_result = pool.get_q4w(&e);
            assert_eq!(first_result, first_q4w_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage::set_pool_q4w(&e, &pool_addr, &1);
            let cached_result = pool.get_q4w(&e);
            assert_eq!(cached_result, first_q4w_amt);

            // new amount gets set and stored
            let second_q4w_amt = 200;
            pool.set_q4w(second_q4w_amt);
            let second_result = pool.get_q4w(&e);
            assert_eq!(second_result, second_q4w_amt);

            // write stores to chain
            pool.write_q4w(&e);
            let chain_result = storage::get_pool_q4w(&e, &pool_addr);
            assert_eq!(chain_result, second_q4w_amt);
        });
    }

    #[test]
    fn test_token_cache() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool::new(&e, pool_addr.clone());

        let first_token_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage::set_pool_tokens(&e, &pool_addr, &first_token_amt);
            let first_result = pool.get_tokens(&e);
            assert_eq!(first_result, first_token_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage::set_pool_tokens(&e, &pool_addr, &1);
            let cached_result = pool.get_tokens(&e);
            assert_eq!(cached_result, first_token_amt);

            // new amount gets set and stored
            let second_token_amt = 200;
            pool.set_tokens(second_token_amt);
            let second_result = pool.get_tokens(&e);
            assert_eq!(second_result, second_token_amt);

            // write stores to chain
            pool.write_tokens(&e);
            let chain_result = storage::get_pool_tokens(&e, &pool_addr);
            assert_eq!(chain_result, second_token_amt);
        });
    }

    #[test]
    fn test_emission_cache() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool::new(&e, pool_addr.clone());

        let first_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage::set_pool_emis(&e, &pool_addr, &first_amt);
            let first_result = pool.get_emissions(&e);
            assert_eq!(first_result, first_amt);

            // cached version returned
            storage::set_pool_emis(&e, &pool_addr, &1);
            let cached_result = pool.get_emissions(&e);
            assert_eq!(cached_result, first_amt);

            // new amount gets set and stored
            let second_amt = 200;
            pool.set_emissions(second_amt);
            let second_result = pool.get_emissions(&e);
            assert_eq!(second_result, second_amt);

            // write stores to chain
            pool.write_emissions(&e);
            let chain_result = storage::get_pool_emis(&e, &pool_addr);
            assert_eq!(chain_result, second_amt);
        });
    }

    /********** Logic **********/

    #[test]
    fn test_convert_to_shares_no_shares() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(0),
            tokens: Some(0),
            q4w: Some(0),
            emissions: Some(0),
        };

        let to_convert = 1234567;
        let shares = pool.convert_to_shares(&e, to_convert);
        assert_eq!(shares, to_convert);
    }

    #[test]
    fn test_convert_to_shares() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(80321),
            tokens: Some(103302),
            q4w: Some(0),
            emissions: Some(0),
        };

        let to_convert = 1234567;
        let shares = pool.convert_to_shares(&e, to_convert);
        assert_eq!(shares, 959920);
    }

    #[test]
    fn test_convert_to_tokens_no_shares() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(0),
            tokens: Some(0),
            q4w: Some(0),
            emissions: Some(0),
        };

        let to_convert = 1234567;
        let shares = pool.convert_to_tokens(&e, to_convert);
        assert_eq!(shares, to_convert);
    }

    #[test]
    fn test_convert_to_tokens() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(80321),
            tokens: Some(103302),
            q4w: Some(0),
            emissions: Some(0),
        };

        let to_convert = 40000;
        let shares = pool.convert_to_tokens(&e, to_convert);
        assert_eq!(shares, 51444);
    }

    #[test]
    fn test_deposit() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
            emissions: Some(0),
        };

        pool.deposit(&e, 50, 25);

        assert_eq!(pool.get_shares(&e), 125);
        assert_eq!(pool.get_tokens(&e), 250);
        assert_eq!(pool.get_q4w(&e), 25);
    }

    #[test]
    fn test_withdraw() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
            emissions: Some(0),
        };

        pool.withdraw(&e, 50, 25).unwrap();

        assert_eq!(pool.get_shares(&e), 75);
        assert_eq!(pool.get_tokens(&e), 150);
        assert_eq!(pool.get_q4w(&e), 0);
    }

    #[test]
    fn test_withdraw_too_much() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
            emissions: Some(0),
        };

        let result = pool.withdraw(&e, 201, 25);

        match result {
            Ok(_) => assert!(false),
            Err(err) => assert_eq!(err, BackstopError::InsufficientFunds),
        }
    }

    #[test]
    fn test_q4w() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
            emissions: Some(0),
        };

        pool.withdraw(&e, 50, 25).unwrap();

        assert_eq!(pool.get_shares(&e), 75);
        assert_eq!(pool.get_tokens(&e), 150);
        assert_eq!(pool.get_q4w(&e), 0);
    }

    #[test]
    fn test_claim() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
            emissions: Some(123),
        };

        pool.claim(&e, 100).unwrap();
        assert_eq!(pool.get_emissions(&e), 23);
    }

    #[test]
    fn test_claim_too_much() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            contract_id: pool_addr.clone(),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
            emissions: Some(123),
        };

        let result = pool.claim(&e, 124);
        match result {
            Ok(_) => assert!(false),
            Err(err) => assert_eq!(err, BackstopError::InsufficientFunds),
        }
    }
}
