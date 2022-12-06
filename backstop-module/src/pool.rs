use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env};

use crate::storage::{BackstopDataStore, StorageManager};

/// A user of the backstop module with respect to a given pool
/// Data is lazy loaded as not all struct information is required for each action
pub struct Pool {
    pub address: BytesN<32>,
    pub id: Identifier,
    shares: Option<u64>,
    tokens: Option<u64>,
    q4w: Option<u64>,
}

impl Pool {
    pub fn new(address: BytesN<32>) -> Pool {
        Pool {
            address: address.clone(),
            id: Identifier::Contract(address),
            shares: None,
            tokens: None,
            q4w: None,
        }
    }

    /********** Setters / Lazy Getters / Storage **********/

    /// Get the pool's total issued shares from the cache or the ledger
    pub fn get_shares(&mut self, e: &Env) -> u64 {
        match self.shares {
            Some(bal) => bal,
            None => {
                let bal = StorageManager::new(e).get_pool_shares(self.address.clone());
                self.shares = Some(bal);
                bal
            }
        }
    }

    /// Set the pool's total issued shares to the cache
    ///
    /// ### Arguments
    /// * `shares` - The pool's total issued shares
    pub fn set_shares(&mut self, shares: u64) {
        self.shares = Some(shares)
    }

    /// Write the currently cached pool's total issued shares to the ledger
    pub fn write_shares(&self, e: &Env) {
        match self.shares {
            Some(bal) => StorageManager::new(e).set_pool_shares(self.address.clone(), bal),
            None => panic!("nothing to write"),
        }
    }

    /// Get the pool's total queued for withdraw from the cache or the ledger
    pub fn get_q4w(&mut self, e: &Env) -> u64 {
        match self.q4w.clone() {
            Some(q4w) => q4w,
            None => {
                let q4w = StorageManager::new(e).get_pool_q4w(self.address.clone());
                self.q4w = Some(q4w);
                q4w
            }
        }
    }

    /// Set the pool's total queued for withdraw to the cache
    ///
    /// ### Arguments
    /// * `q4w` - The pool's total queued for withdraw
    pub fn set_q4w(&mut self, q4w: u64) {
        self.q4w = Some(q4w)
    }

    /// Write the currently cached pool's total queued for withdraw to the ledger
    pub fn write_q4w(&self, e: &Env) {
        match self.q4w {
            Some(q4w) => StorageManager::new(e).set_pool_q4w(self.address.clone(), q4w),
            None => panic!("nothing to write"),
        }
    }

    /// Get the pool's total backstop tokens from the cache or the ledger
    pub fn get_tokens(&mut self, e: &Env) -> u64 {
        match self.tokens {
            Some(bal) => bal,
            None => {
                let bal = StorageManager::new(e).get_pool_tokens(self.address.clone());
                self.tokens = Some(bal);
                bal
            }
        }
    }

    /// Set the pool's total backstop tokens to the cache
    ///
    /// ### Arguments
    /// * `tokens` - The pool's backstop tokens
    pub fn set_tokens(&mut self, tokens: u64) {
        self.tokens = Some(tokens)
    }

    /// Write the currently cached pool's total backstop tokens to the ledger
    pub fn write_tokens(&self, e: &Env) {
        match self.tokens {
            Some(bal) => StorageManager::new(e).set_pool_tokens(self.address.clone(), bal),
            None => panic!("nothing to write"),
        }
    }

    /********** Logic **********/

    /// Convert a token balance to a share balance based on the current pool state
    ///
    /// ### Arguments
    /// * `tokens` - the token balance to convert
    pub fn convert_to_shares(&mut self, e: &Env, tokens: u64) -> u64 {
        let pool_shares = self.get_shares(e);
        if pool_shares == 0 {
            return tokens;
        }

        (tokens * pool_shares) / self.get_tokens(e)
    }

    /// Convert a pool share balance to a token balance based on the current pool state
    ///
    /// ### Arguments
    /// * `shares` - the pool share balance to convert
    pub fn convert_to_tokens(&mut self, e: &Env, shares: u64) -> u64 {
        let pool_shares = self.get_shares(e);
        if pool_shares == 0 {
            return shares;
        }

        (shares * self.get_tokens(e)) / pool_shares
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
    pub fn deposit(&mut self, e: &Env, tokens: u64, shares: u64) {
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
    pub fn withdraw(&mut self, e: &Env, tokens: u64, shares: u64) {
        let cur_tokens = self.get_tokens(e);
        let cur_shares = self.get_shares(e);
        let cur_q4w = self.get_q4w(e);
        self.set_tokens(cur_tokens - tokens);
        self.set_shares(cur_shares - shares);
        self.set_q4w(cur_q4w - shares);
    }

    /// Queue withdraw for the pool
    ///
    /// Updates cached values but does not write:
    /// * q4w
    ///
    /// ### Arguments
    /// * `shares` - The amount of shares to queue for withdraw
    pub fn queue_for_withdraw(&mut self, e: &Env, shares: u64) {
        let cur_q4w = self.get_q4w(e);
        self.set_q4w(cur_q4w + shares);
    }
}

#[cfg(test)]
mod tests {
    use crate::testutils::generate_contract_id;

    use super::*;

    /********** Cache / Getters / Setters **********/

    #[test]
    fn test_share_cache() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool::new(pool_addr.clone());

        let first_share_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage.set_pool_shares(pool_addr.clone(), first_share_amt.clone());
            let first_result = pool.get_shares(&e);
            assert_eq!(first_result, first_share_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage.set_pool_shares(pool_addr.clone(), 1);
            let cached_result = pool.get_shares(&e);
            assert_eq!(cached_result, first_share_amt);

            // new amount gets set and stored
            let second_share_amt = 200;
            pool.set_shares(second_share_amt);
            let second_result = pool.get_shares(&e);
            assert_eq!(second_result, second_share_amt);

            // write stores to chain
            pool.write_shares(&e);
            let chain_result = storage.get_pool_shares(pool_addr);
            assert_eq!(chain_result, second_share_amt);
        });
    }

    #[test]
    fn test_q4w_cache() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool::new(pool_addr.clone());

        let first_q4w_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage.set_pool_q4w(pool_addr.clone(), first_q4w_amt.clone());
            let first_result = pool.get_q4w(&e);
            assert_eq!(first_result, first_q4w_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage.set_pool_q4w(pool_addr.clone(), 1);
            let cached_result = pool.get_q4w(&e);
            assert_eq!(cached_result, first_q4w_amt);

            // new amount gets set and stored
            let second_q4w_amt = 200;
            pool.set_q4w(second_q4w_amt);
            let second_result = pool.get_q4w(&e);
            assert_eq!(second_result, second_q4w_amt);

            // write stores to chain
            pool.write_q4w(&e);
            let chain_result = storage.get_pool_q4w(pool_addr);
            assert_eq!(chain_result, second_q4w_amt);
        });
    }

    #[test]
    fn test_token_cache() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool::new(pool_addr.clone());

        let first_token_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage.set_pool_tokens(pool_addr.clone(), first_token_amt.clone());
            let first_result = pool.get_tokens(&e);
            assert_eq!(first_result, first_token_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage.set_pool_tokens(pool_addr.clone(), 1);
            let cached_result = pool.get_tokens(&e);
            assert_eq!(cached_result, first_token_amt);

            // new amount gets set and stored
            let second_token_amt = 200;
            pool.set_tokens(second_token_amt);
            let second_result = pool.get_tokens(&e);
            assert_eq!(second_result, second_token_amt);

            // write stores to chain
            pool.write_tokens(&e);
            let chain_result = storage.get_pool_tokens(pool_addr);
            assert_eq!(chain_result, second_token_amt);
        });
    }

    /********** Logic **********/

    #[test]
    fn test_convert_to_shares_no_shares() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            address: pool_addr.clone(),
            id: Identifier::Contract(pool_addr),
            shares: Some(0),
            tokens: Some(0),
            q4w: Some(0),
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
            address: pool_addr.clone(),
            id: Identifier::Contract(pool_addr),
            shares: Some(80321),
            tokens: Some(103302),
            q4w: Some(0),
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
            address: pool_addr.clone(),
            id: Identifier::Contract(pool_addr),
            shares: Some(0),
            tokens: Some(0),
            q4w: Some(0),
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
            address: pool_addr.clone(),
            id: Identifier::Contract(pool_addr),
            shares: Some(80321),
            tokens: Some(103302),
            q4w: Some(0),
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
            address: pool_addr.clone(),
            id: Identifier::Contract(pool_addr),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
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
            address: pool_addr.clone(),
            id: Identifier::Contract(pool_addr),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
        };

        pool.withdraw(&e, 50, 25);

        assert_eq!(pool.get_shares(&e), 75);
        assert_eq!(pool.get_tokens(&e), 150);
        assert_eq!(pool.get_q4w(&e), 0);
    }

    #[test]
    fn test_q4w() {
        let e = Env::default();
        let pool_addr = generate_contract_id(&e);
        let mut pool = Pool {
            address: pool_addr.clone(),
            id: Identifier::Contract(pool_addr),
            shares: Some(100),
            tokens: Some(200),
            q4w: Some(25),
        };

        pool.withdraw(&e, 50, 25);

        assert_eq!(pool.get_shares(&e), 75);
        assert_eq!(pool.get_tokens(&e), 150);
        assert_eq!(pool.get_q4w(&e), 0);
    }
}
