use crate::storage;
use soroban_sdk::{contractimpl, BytesN, Env, RawVal, Symbol, Vec};

pub struct PoolFactory;

pub trait PoolFactoryTrait {
    /// Setup the pool factory
    ///
    /// ### Arguments
    /// * `wasm_hash` - The WASM hash of the lending pool's WASM code
    fn initialize(e: Env, wasm_hash: BytesN<32>);

    /// Deploys and initializes a lending pool
    ///
    /// # Arguments
    /// * 'init_function' - The name of the pool's initialization function
    /// * 'args' - The vectors of args for pool initialization
    fn deploy(e: Env, init_function: Symbol, args: Vec<RawVal>) -> BytesN<32>;

    /// Checks if contract address was deployed by the factory
    ///
    /// Returns true if pool was deployed by factory and false otherwise
    ///
    /// # Arguments
    /// * 'pool_address' - The contract address to be checked
    fn is_pool(e: Env, pool_address: BytesN<32>) -> bool;
}

#[contractimpl]
impl PoolFactoryTrait for PoolFactory {
    fn initialize(e: Env, wasm_hash: BytesN<32>) {
        if storage::has_wasm_hash(&e) {
            panic!("already initialized");
        }
        storage::set_wasm_hash(&e, &wasm_hash);
    }

    fn deploy(e: Env, init_function: Symbol, args: Vec<RawVal>) -> BytesN<32> {
        let mut salt: [u8; 32] = [0; 32];
        let sequence_as_bytes = e.ledger().sequence().to_be_bytes();
        for n in 0..sequence_as_bytes.len() {
            salt[n] = sequence_as_bytes[n];
        }

        let pool_address = e
            .deployer()
            .with_current_contract(&salt)
            .deploy(&storage::get_wasm_hash(&e));
        // e.invoke_contract::<RawVal>(&pool_address, &init_function, args);

        storage::set_deployed(&e, &pool_address);

        e.events()
            .publish((Symbol::new(&e, "deploy"),), pool_address.clone());
        pool_address
    }

    fn is_pool(e: Env, pool_address: BytesN<32>) -> bool {
        storage::is_deployed(&e, &pool_address)
    }
}
