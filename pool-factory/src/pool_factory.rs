use crate::storage::{self, PoolInitMeta};
use soroban_sdk::{contractimpl, vec, Address, BytesN, Env, IntoVal, RawVal, Symbol, Vec};

pub struct PoolFactory;

pub trait PoolFactoryTrait {
    /// Setup the pool factory
    ///
    /// ### Arguments
    /// * `pool_init_meta` - The pool initialization metadata
    fn initialize(e: Env, pool_init_meta: PoolInitMeta);

    /// Deploys and initializes a lending pool
    ///
    /// # Arguments
    /// * `admin` - The admin address for the pool
    /// * `oracle` - The oracle BytesN<32> ID for the pool
    /// * `backstop_take_rate` - The backstop take rate for the pool
    fn deploy(
        e: Env,
        admin: Address,
        salt: BytesN<32>,
        oracle: BytesN<32>,
        backstop_take_rate: u64,
    ) -> BytesN<32>;

    /// Checks if contract address was deployed by the factory
    ///
    /// Returns true if pool was deployed by factory and false otherwise
    ///
    /// # Arguments
    /// * `pool_id` - The contract BytesN<32> ID to be checked
    fn is_pool(e: Env, pool_id: BytesN<32>) -> bool;
}

#[contractimpl]
impl PoolFactoryTrait for PoolFactory {
    fn initialize(e: Env, pool_init_meta: PoolInitMeta) {
        if storage::has_pool_init_meta(&e) {
            panic!("already initialized");
        }
        storage::set_pool_init_meta(&e, &pool_init_meta);
        storage::set_salt(&e, &0);
    }

    fn deploy(
        e: Env,
        admin: Address,
        salt: BytesN<32>,
        oracle: BytesN<32>,
        backstop_take_rate: u64,
    ) -> BytesN<32> {
        let pool_init_meta = storage::get_pool_init_meta(&e);

        let mut init_args: Vec<RawVal> = vec![&e];
        init_args.push_back(admin.to_raw());
        init_args.push_back(oracle.to_raw());
        init_args.push_back(backstop_take_rate.into_val(&e));
        init_args.push_back(pool_init_meta.backstop.to_raw());
        init_args.push_back(pool_init_meta.b_token_hash.to_raw());
        init_args.push_back(pool_init_meta.d_token_hash.to_raw());
        init_args.push_back(pool_init_meta.blnd_id.to_raw());
        init_args.push_back(pool_init_meta.usdc_id.to_raw());
        let pool_address = e
            .deployer()
            .with_current_contract(&salt)
            .deploy(&pool_init_meta.pool_hash);
        e.invoke_contract::<RawVal>(&pool_address, &Symbol::new(&e, "initialize"), init_args);

        storage::set_deployed(&e, &pool_address);

        e.events()
            .publish((Symbol::new(&e, "deploy"),), pool_address.clone());
        pool_address
    }

    fn is_pool(e: Env, pool_address: BytesN<32>) -> bool {
        storage::is_deployed(&e, &pool_address)
    }
}
