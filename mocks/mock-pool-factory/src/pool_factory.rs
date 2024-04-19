use crate::{
    storage::{self, PoolInitMeta},
    PoolFactoryError,
};
use soroban_sdk::{
    contract, contractimpl, panic_with_error, vec, Address, BytesN, Env, IntoVal, String, Symbol,
    Val, Vec,
};

use pool::PoolContract;

#[contract]
pub struct MockPoolFactory;

pub trait MockPoolFactoryTrait {
    /// Setup the pool factory
    ///
    /// ### Arguments
    /// * `pool_init_meta` - The pool initialization metadata
    fn initialize(e: Env, pool_init_meta: PoolInitMeta);

    /// Deploys and initializes a lending pool
    ///
    /// # Arguments
    /// * `admin` - The admin address for the pool
    /// * `name` - The name of the pool
    /// * `oracle` - The oracle address for the pool
    /// * `backstop_take_rate` - The backstop take rate for the pool (7 decimals)
    fn deploy(
        e: Env,
        admin: Address,
        name: String,
        salt: BytesN<32>,
        oracle: Address,
        backstop_take_rate: u32,
        max_positions: u32,
    ) -> Address;

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
    fn initialize(e: Env, pool_init_meta: PoolInitMeta) {
        if storage::has_pool_init_meta(&e) {
            panic_with_error!(&e, PoolFactoryError::AlreadyInitialized);
        }
        storage::set_pool_init_meta(&e, &pool_init_meta);
    }

    fn deploy(
        e: Env,
        admin: Address,
        name: String,
        _salt: BytesN<32>,
        oracle: Address,
        backstop_take_rate: u32,
        max_positions: u32,
    ) -> Address {
        storage::extend_instance(&e);
        admin.require_auth();
        let pool_init_meta = storage::get_pool_init_meta(&e);

        // verify backstop take rate is within [0,1) with 9 decimals
        if backstop_take_rate >= 1_0000000 {
            panic_with_error!(&e, PoolFactoryError::InvalidPoolInitArgs);
        }

        let mut init_args: Vec<Val> = vec![&e];
        init_args.push_back(admin.to_val());
        init_args.push_back(name.to_val());
        init_args.push_back(oracle.to_val());
        init_args.push_back(backstop_take_rate.into_val(&e));
        init_args.push_back(max_positions.into_val(&e));
        init_args.push_back(pool_init_meta.backstop.to_val());
        init_args.push_back(pool_init_meta.blnd_id.to_val());

        let pool_address = e.register_contract(None, PoolContract {});
        e.invoke_contract::<Val>(&pool_address, &Symbol::new(&e, "initialize"), init_args);

        storage::set_deployed(&e, &pool_address);

        e.events()
            .publish((Symbol::new(&e, "deploy"),), pool_address.clone());
        pool_address
    }

    fn is_pool(e: Env, pool_address: Address) -> bool {
        storage::is_deployed(&e, &pool_address)
    }

    fn set_pool(e: Env, pool_address: Address) {
        storage::set_deployed(&e, &pool_address);
    }
}
