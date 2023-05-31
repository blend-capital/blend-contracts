use crate::storage;
use soroban_sdk::{contractimpl, Address, Env};

pub struct MockPoolFactory;

pub trait MockPoolFactoryTrait {
    /// Checks if contract address was deployed by the factory
    ///
    /// Returns true if pool was deployed by factory and false otherwise
    ///
    /// # Arguments
    /// * 'pool_address' - The contract address to be checked
    fn is_pool(e: Env, pool_address: Address) -> bool;

    /// Mock Only: Set a pool_address as having been deployed by the pool factory
    ///
    /// ### Arguments
    /// * `pool_address` - The pool address to set
    fn set_pool(e: Env, pool_address: Address);
}

#[contractimpl]
impl MockPoolFactoryTrait for MockPoolFactory {
    fn is_pool(e: Env, pool_address: Address) -> bool {
        storage::is_deployed(&e, &pool_address)
    }

    fn set_pool(e: Env, pool_address: Address) {
        storage::set_deployed(&e, &pool_address);
    }
}
