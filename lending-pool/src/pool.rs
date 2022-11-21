use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, BytesN, Env, BigInt};
use crate::{
    dependencies::TokenClient,
    storage::{StorageManager, PoolDataStore, ReserveConfig, ReserveData}, errors::PoolError, user_validator::validate_hf, user_data::UserAction, user_config::{UserConfigurator, UserConfig}, 
};

const SCALAR: i64 = 1_000_000_0;

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

    fn config(e: Env, user: Identifier) -> u64;

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
    fn supply(e: Env, asset: BytesN<32>, amount: BigInt) -> Result<BigInt, PoolError>;

    /// Withdraws `amount` of the `asset` from the invoker and returns it to the `to` Identifier
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
    fn withdraw(e: Env, asset: BytesN<32>, amount: BigInt, to: Identifier) -> Result<BigInt, PoolError>;

    /// Borrow's `amount` of `asset` from the pool and sends it to the `to` address and credits a debt
    /// to the invoker
    /// 
    /// Returns the amount of dTokens minted
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to borrow
    /// * `to` - The address receiving the funds
    fn borrow(e: Env, asset: BytesN<32>, amount: BigInt, to: Identifier) -> Result<BigInt, PoolError>;

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
    fn repay(e: Env, asset: BytesN<32>, amount: BigInt, on_behalf_of: Identifier) -> Result<BigInt, PoolError>;
}

#[contractimpl]
impl PoolTrait for Pool {
    fn initialize(e: Env, admin: Identifier, oracle: BytesN<32>) {
        let storage = StorageManager::new(&e);
        if storage.has_admin() {
            panic!("already initialized")
        }

        storage.set_admin(admin);
        storage.set_oracle(oracle);
    }

    // @dev: This function will be reworked - used for testing purposes
    fn init_res(e: Env, asset: BytesN<32>, config: ReserveConfig) {
        let storage = StorageManager::new(&e);

        if storage.has_res(asset.clone()) {
            panic!("already initialized")
        }

        if Identifier::from(e.invoker()) != storage.get_admin() {
            panic!("not authorized")
        }

        storage.set_res_config(asset.clone(), config);
        let init_data = ReserveData {
            b_rate: 1_100_000_0, // TODO: revert this, currently set for testing
            d_rate: 1_200_000_0, // TODO: revert this, currently set for testing
            ir_mod: 1_000_000_0,
        };
        storage.set_res_data(asset, init_data);
    }

    fn config(e: Env, user: Identifier) -> u64 {
        let storage = StorageManager::new(&e);
        storage.get_user_config(user)
    }

    fn supply(e: Env, asset: BytesN<32>, amount: BigInt) -> Result<BigInt, PoolError> {
        let storage = StorageManager::new(&e);

        let res_config = storage.get_res_config(asset.clone());
        let res_data = storage.get_res_data(asset.clone());

        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);
        let to_mint = (amount.clone() * BigInt::from_i64(&e, SCALAR)) / BigInt::from_i64(&e, res_data.b_rate);

        TokenClient::new(&e, asset).xfer_from(
            &Signature::Invoker, 
            &BigInt::zero(&e), 
            &invoker_id, 
            &get_contract_id(&e), 
            &amount
        );

        TokenClient::new(&e, res_config.b_token).mint(
            &Signature::Invoker, 
            &BigInt::zero(&e), 
            &invoker_id, 
            &to_mint
        );

        let mut user_config = UserConfig::new(storage.get_user_config(invoker_id.clone()));
        if !user_config.is_collateral(res_config.index) {
            user_config.set_collateral(res_config.index, true);
            storage.set_user_config(invoker_id, user_config.config);
        }

        // TODO: rate/index updates
        Ok(to_mint)
    }

    fn withdraw(e: Env, asset: BytesN<32>, amount: BigInt, to: Identifier) -> Result<BigInt, PoolError> {
        let storage = StorageManager::new(&e);

        let res_config = storage.get_res_config(asset.clone());
        let res_data = storage.get_res_data(asset.clone());

        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);
        let to_burn: BigInt;
        let to_return: BigInt;
        let b_token_client = TokenClient::new(&e, res_config.b_token);
        if amount == BigInt::from_u64(&e, u64::MAX) {
            // if they input u64::MAX as the burn amount, burn 100% of their holdings
            to_burn = b_token_client.balance(&invoker_id);
            to_return = (to_burn.clone() * BigInt::from_i64(&e, res_data.b_rate)) / BigInt::from_i64(&e, SCALAR);
        } else {
            to_burn = (amount.clone() * BigInt::from_i64(&e, SCALAR)) / BigInt::from_i64(&e, res_data.b_rate);
            to_return = amount;
        }

        let user_action = UserAction {
            asset: asset.clone(),
            b_token_delta: -(to_burn.to_i64()),
            d_token_delta: 0
        };
        let is_healthy = validate_hf(&e, &invoker_id, &user_action);
        if !is_healthy {
            return Err(PoolError::InvalidHf);
        }

        b_token_client.burn(
            &Signature::Invoker, 
            &BigInt::zero(&e), 
            &invoker_id, 
            &to_burn
        );

        TokenClient::new(&e, asset).xfer(
            &Signature::Invoker, 
            &BigInt::zero(&e),
            &to, 
            &to_return
        );

        let mut user_config = UserConfig::new(storage.get_user_config(invoker_id.clone()));
        if b_token_client.balance(&invoker_id).is_zero() {
            user_config.set_collateral(res_config.index, false);
            storage.set_user_config(invoker_id, user_config.config);
        }
        // TODO: rate/index updates
        Ok(to_burn)
    }

    fn borrow(e: Env, asset: BytesN<32>, amount: BigInt, to: Identifier) -> Result<BigInt, PoolError> {
        let storage = StorageManager::new(&e);

        let res_config = storage.get_res_config(asset.clone());
        let res_data = storage.get_res_data(asset.clone());

        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);
        let to_mint = (amount.clone() * BigInt::from_i64(&e, SCALAR)) / BigInt::from_i64(&e, res_data.d_rate);

        let user_action = UserAction {
            asset: asset.clone(),
            b_token_delta: 0,
            d_token_delta: to_mint.to_i64()
        };
        let is_healthy = validate_hf(&e, &invoker_id, &user_action);
        if !is_healthy {
            return Err(PoolError::InvalidHf);
        }

        TokenClient::new(&e, res_config.d_token).mint(
            &Signature::Invoker, 
            &BigInt::zero(&e), 
            &invoker_id, 
            &to_mint
        );
        
        TokenClient::new(&e, asset).xfer(
            &Signature::Invoker, 
            &BigInt::zero(&e), 
            &to, 
            &amount
        );

        let mut user_config = UserConfig::new(storage.get_user_config(invoker_id.clone()));
        if !user_config.is_borrowing(res_config.index) {
            user_config.set_borrowing(res_config.index, true);
            storage.set_user_config(invoker_id, user_config.config);
        }

        // TODO: rate/index updates
        Ok(to_mint)
    }

    fn repay(e: Env, asset: BytesN<32>, amount: BigInt, on_behalf_of: Identifier) -> Result<BigInt, PoolError>{
        let storage = StorageManager::new(&e);

        let res_config = storage.get_res_config(asset.clone());
        let res_data = storage.get_res_data(asset.clone());

        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);
        let to_burn: BigInt;
        let to_repay: BigInt;
        let d_token_client = TokenClient::new(&e, res_config.d_token);
        if amount == BigInt::from_u64(&e, u64::MAX) {
            // if they input u64::MAX as the repay amount, burn 100% of their holdings
            to_burn = d_token_client.balance(&invoker_id);
            to_repay = (to_burn.clone() *  BigInt::from_i64(&e, res_data.d_rate)) / BigInt::from_i64(&e, SCALAR);
        } else {
            to_burn = (amount.clone() * BigInt::from_i64(&e, SCALAR)) / BigInt::from_i64(&e, res_data.d_rate);
            to_repay = amount;
        }

        d_token_client.burn(
            &Signature::Invoker, 
            &BigInt::zero(&e), 
            &on_behalf_of, 
            &to_burn
        );

        TokenClient::new(&e, asset).xfer_from(
            &Signature::Invoker, 
            &BigInt::zero(&e),
            &invoker_id,
            &get_contract_id(&e),
            &to_repay
        );

        let mut user_config = UserConfig::new(storage.get_user_config(invoker_id.clone()));
        if d_token_client.balance(&invoker_id).is_zero() {
            user_config.set_borrowing(res_config.index, false);
            storage.set_user_config(invoker_id, user_config.config);
        }

        // TODO: rate/index updates
        Ok(to_burn)
    }
}

// ****** Helpers *****

fn get_contract_id(e: &Env) -> Identifier {
    Identifier::Contract(e.current_contract())
}
