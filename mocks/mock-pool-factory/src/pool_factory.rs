use crate::storage::{self, PoolFactoryStore, StorageManager};
use soroban_sdk::{contractimpl, BytesN, Env, RawVal, Symbol, Vec};

pub struct MockPoolFactory;

pub trait MockPoolFactoryTrait {
    /// Checks if contract address was deployed by the factory
    ///
    /// Returns true if pool was deployed by factory and false otherwise
    ///
    /// # Arguments
    /// * 'pool_address' - The contract address to be checked
    fn is_pool(e: Env, pool_address: BytesN<32>) -> bool;

    /// Mock Only: Set a pool_address as having been deployed by the pool factory
    ///
    /// ### Arguments
    /// * `pool_address` - The pool address to set
    fn set_pool(e: Env, pool_address: BytesN<32>);
}

#[contractimpl]
impl MockPoolFactoryTrait for MockPoolFactory {
    fn is_pool(e: Env, pool_address: BytesN<32>) -> bool {
        let storage = StorageManager::new(&e);
        storage.is_deployed(pool_address)
    }

    fn set_pool(e: Env, pool_address: BytesN<32>) {
        let storage = StorageManager::new(&e);
        storage.set_deployed(pool_address);
    }
}
