use soroban_auth::Identifier;
use soroban_sdk::{contractimpl, contracttype, Address, BytesN, Env, contracterror, BigInt};
use crate::{dependencies::{OracleClient}, types::{ReserveConfig, ReserveData}, storage};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PoolError {
    StaleOracle = 1,
}

/// ### Pool
///
/// An isolated money market pool.
pub struct Pool;

pub trait PoolTrait {
    /// Initialize the pool
    /// 
    /// ### Arguments
    /// * `admin` - The identifier for the admin account
    /// * `oracle` - The contract address of the oracle
    fn initialize(e: Env, admin: Identifier, oracle: BytesN<32>);

    /// Initialize a reserve in the pool
    /// 
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    /// * `config` - The ReserveConfig for the reserve
    /// 
    /// ### Errors
    /// If the caller is not the admin
    fn init_res(e: Env, asset: BytesN<32>, config: ReserveConfig);

    /// Invoker supplies the `amount` of `asset` into the pool in return for the asset's bToken
    /// 
    /// Returns the amount of bTokens minted
    ///
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to supply
    ///
    /// ### Errors
    /// If the invoker has not approved the pool to transfer `asset` at least `amount` and has 
    /// enough tokens to do so
    fn supply(e: Env, asset: BytesN<32>) -> Result<BigInt, PoolError>;

    /// Withdraws `amount` of the `asset` from the invoker and returns it to the `to` Address
    /// 
    /// Returns the amount of bTokens burnt
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to withdraw
    /// * `to` - The address to send the withdrawn funds to
    /// 
    /// ### Errors
    /// If the invoker does not have enough funds to burn
    fn withdraw(e: Env, asset: BytesN<32>, amount: BigInt, to: Address) -> Result<BigInt, PoolError>;

    /// Borrow's `amount` of `asset` from the pool and sends it to the `to` address and credits a debt
    /// to the invoker
    /// 
    /// Returns the amount of lTokens minted
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to borrow
    /// * `to` - The address receiving the funds
    fn borrow(e: Env, asset: BytesN<32>, amount: BigInt, to: Address) -> Result<BigInt, PoolError>;

    /// Invoker repays the `amount` of debt for the `asset`, such that the debt is reduced for
    /// the address `on_behalf_of`
    /// 
    /// Returns the amount of lTokens burned
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to borrow
    ///     * Sending u64.MAX will repay the full amount of the debt
    /// * `on_behalf_of` - The address receiving the funds
    fn repay(e: Env, asset: BytesN<32>, amount: BigInt, on_behalf_of: Address) -> Result<BigInt, PoolError>;
}

#[contractimpl]
impl PoolTrait for Pool {
    fn initialize(e: Env, admin: Identifier, oracle: BytesN<32>) {
        if storage::has_admin(&e) {
            panic!("already initialized")
        }

        storage::set_admin(&e, admin);
        storage::set_oracle(&e, oracle);
    }

    // @dev: This function will be reworked - used for testing purposes
    fn init_res(e: Env, asset: BytesN<32>, config: ReserveConfig) {
        if storage::has_res(&e, asset.clone()) {
            panic!("already initialized")
        }

        storage::set_res_config(&e, asset.clone(), config);
        let init_data = ReserveData {
            rate: 1_000_000_0,
            ir_mod: 1_000_000_0,
        };
        storage::set_res_data(&e, asset, init_data)
    }

    fn supply(e: Env, asset: BytesN<32>) -> Result<BigInt, PoolError> {
        panic!("not impl")
    }

    fn withdraw(e: Env, asset: BytesN<32>, amount: BigInt, to: Address) -> Result<BigInt, PoolError> {
        panic!("not impl")
    }

    fn borrow(e: Env, asset: BytesN<32>, amount: BigInt, to: Address) -> Result<BigInt, PoolError> {
        panic!("not impl")
    }

    fn repay(e: Env, asset: BytesN<32>, amount: BigInt, on_behalf_of: Address) -> Result<BigInt, PoolError>{
        panic!("not impl")
    }
}
