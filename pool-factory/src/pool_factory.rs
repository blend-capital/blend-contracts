use crate::storage::{PoolFactoryDataKey, PoolFactoryStore, StorageManager};
use soroban_sdk::{contractimpl, Bytes, BytesN, Env, RawVal, Symbol, Vec};

pub struct PoolFactory;

pub trait PoolFactoryTrait {
    fn initialize(e: Env, wasm: Bytes);

    /// Deploys and initalizes a lending pool
    ///
    /// # Arguments
    /// * 'wasm' - The lending pool wasm blob
    /// * 'salt' - The salt for deployment
    /// * 'init_function' - The name of the pool's initialization function
    /// * 'args' - The vectors of args for pool initialization
    fn deploy(e: Env, init_function: Symbol, args: Vec<RawVal>) -> BytesN<32>;

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
    fn initialize(e: Env, wasm: Bytes) {
        let storage = StorageManager::new(&e);
        if storage.has_wasm() {
            panic!("Already initalized");
        }
        storage.set_wasm(wasm);
    }

    fn deploy(e: Env, init_function: Symbol, args: Vec<RawVal>) -> BytesN<32> {
        let storage = StorageManager::new(&e);
        let mut salt: [u8; 32] = [0; 32];
        let sequence_as_bytes = e.ledger().sequence().to_be_bytes();

        for n in 0..sequence_as_bytes.len() {
            salt[n] = sequence_as_bytes[n];
        }

        let pool_address = e
            .deployer()
            .with_current_contract(salt)
            .deploy(storage.get_wasm());
        e.invoke_contract::<RawVal>(&pool_address, &init_function, args);
        storage.set_deployed(pool_address.clone());
        pool_address
    }

    fn is_deploy(e: Env, pool_address: BytesN<32>) -> bool {
        let storage = StorageManager::new(&e);
        storage.get_deployed(pool_address)
    }
}
