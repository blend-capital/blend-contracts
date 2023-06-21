use soroban_sdk::{Address, Map, Env, map, panic_with_error};

use crate::{storage::{PoolConfig, self}, errors::PoolError};

use super::reserve::Reserve;

pub struct Pool {
    pub config: PoolConfig,
    pub reserves: Map<Address, Reserve>,
}

impl Pool {
    /// Load the Pool from the ledger
    pub fn load(e: &Env) -> Self {
        let pool_config = storage::get_pool_config(e);
        Pool { config: pool_config, reserves: map![&e] }
    }

    /// Load a Reserve from the ledger and update to the current ledger timestamp. Returns
    /// a cached version if it exists.
    /// 
    /// ### Arguments
    /// * asset - The address of the underlying asset
    pub fn load_reserve(&mut self, e: &Env, asset: &Address) -> Reserve {
        if let Some(reserve) = self.reserves.get(asset.clone()) {
            return reserve.unwrap();
        }
        return Reserve::load(e, &self.config, asset);
    }

    /// Cache the updated reserve in the pool.
    /// 
    /// ### Arguments
    /// * reserve - The updated reserve
    pub fn cache_reserve(&mut self, reserve: Reserve) {
        self.reserves.set(reserve.asset.clone(), reserve);
    }

    /// Store the cached reserves to the ledger.
    pub fn store_cached_reserves(&self, e: &Env) {
        for reserve in self.reserves.values().iter_unchecked() {
            reserve.store(e);
        }
    }

    /// Require that the action does not violate the pool status, or panic.
    /// 
    /// ### Arguments
    /// * `action_type` - The type of action being performed
    pub fn require_action_allowed(&self, e: &Env, action_type: u32) {
        // disable borrowing for any non-active pool
        if self.config.status > 0 && action_type == 4 {
            panic_with_error!(e, PoolError::InvalidPoolStatus);
        }
        // disable supplying for any frozen pool
        else if self.config.status > 1 && (action_type == 2 || action_type == 0) {
            panic_with_error!(e, PoolError::InvalidPoolStatus);
        }
    }
}
