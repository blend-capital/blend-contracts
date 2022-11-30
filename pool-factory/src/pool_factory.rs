use crate::storage::{PoolFactoryStore, StorageManager};
use soroban_sdk::{contractimpl, Bytes, BytesN, Env, RawVal, Symbol, Vec};

pub struct PoolFactory;

pub trait PoolFactoryTrait {
    /// Deploys and initalizes a lending pool
    ///
    /// # Arguments
    /// * 'wasm' - The lending pool wasm blob
    /// * 'salt' - The salt for deployment
    /// * 'init_function' - The name of the pool's initialization function
    /// * 'args' - The vectors of args for pool initialization
    fn deploy(
        e: Env,
        wasm: Bytes,
        salt: Bytes,
        init_function: Symbol,
        args: Vec<RawVal>,
    ) -> BytesN<32>;

    /// Checks if contract address was deployed by the factory
    ///
    /// Returns true if pool was deployed by factory and false otherwise
    ///
    /// # Arguments
    /// * 'pool_address' - The contract address to be checked
    fn is_deploy(e: Env, pool_address: BytesN<32>) -> bool;
}

#[contractimpl]
impl PoolFactoryTrait for PoolFactory {
    fn deploy(
        e: Env,
        wasm: Bytes,
        salt: Bytes,
        init_function: Symbol,
        args: Vec<RawVal>,
    ) -> BytesN<32> {
        let storage = StorageManager::new(&e);
        let pool_address = e.deployer().with_current_contract(salt).deploy(wasm);
        e.invoke_contract::<RawVal>(&pool_address, &init_function, args);
        storage.set_deployed(pool_address.clone());
        pool_address
    }

    fn is_deploy(e: Env, pool_address: BytesN<32>) -> bool {
        let storage = StorageManager::new(&e);
        storage.get_deployed(pool_address)
    }
}
