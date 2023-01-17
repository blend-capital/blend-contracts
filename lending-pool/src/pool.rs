use crate::{
    dependencies::{BackstopClient, EmitterClient, TokenClient},
    emissions_distributor,
    emissions_manager::{self, ReserveEmissionMetadata},
    errors::PoolError,
    reserve::Reserve,
    reserve_usage::ReserveUsage,
    storage::{
        PoolDataStore, ReserveConfig, ReserveData, ReserveEmissionsConfig, ReserveEmissionsData,
        StorageManager,
    },
    user_data::UserAction,
    user_validator::validate_hf,
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, Address, BytesN, Env, Map, Vec};

/// ### Pool
///
/// An isolated money market pool.
pub struct Pool;

// Constants
const EMITTER: [u8; 32] = [100; 32]; // TODO: Use the emitter

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

    /// Fetch the reserve usage configuration for a user
    ///
    /// ### Arguments
    /// * `user` - The identifier to fetch the reserve usage for
    fn config(e: Env, user: Identifier) -> u128;

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
    fn supply(e: Env, asset: BytesN<32>, amount: u64) -> Result<u64, PoolError>;

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
    fn withdraw(e: Env, asset: BytesN<32>, amount: u64, to: Identifier) -> Result<u64, PoolError>;

    /// Borrow's `amount` of `asset` from the pool and sends it to the `to` address and credits a debt
    /// to the invoker
    ///
    /// Returns the amount of dTokens minted
    ///
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to borrow
    /// * `to` - The address receiving the funds
    fn borrow(e: Env, asset: BytesN<32>, amount: u64, to: Identifier) -> Result<u64, PoolError>;

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
    fn repay(
        e: Env,
        asset: BytesN<32>,
        amount: u64,
        on_behalf_of: Identifier,
    ) -> Result<u64, PoolError>;

    /// Pool status is changed to 'pool_status" if invoker is the admin
    /// * 0 = active
    /// * 1 = on ice
    /// * 2 = frozen
    ///
    /// ### Arguments
    /// * 'pool_status' - The pool status to be set
    fn set_status(e: Env, pool_status: u32) -> Result<(), PoolError>;

    /// Fetch the status of the pool
    /// * 0 = active
    /// * 1 = on ice
    /// * 2 = frozen
    fn status(e: Env) -> u32;

    /********* Emission Functions **********/

    /// Fetch the next emission configuration
    fn get_emis(e: Env) -> Map<u32, u64>;

    /// Update emissions for reserves for the next emission cycle
    ///
    /// Needs to be performed each emission cycle, as determined by the expiration
    ///
    /// Returns the expiration timestamp
    fn updt_emis(e: Env) -> Result<u64, PoolError>;

    /// Set the emission configuration for the pool
    ///
    /// Changes will be applied in the next pool `updt_emis`, and affect the next emission cycle
    ///
    /// ### Arguments
    /// * `res_emission_metadata` - A vector of ReserveEmissionMetadata to update metadata to
    ///
    /// ### Errors
    /// * If the caller is not the admin
    /// * If the sum of ReserveEmissionMetadata shares is greater than 1
    fn set_emis(
        e: Env,
        res_emission_metadata: Vec<ReserveEmissionMetadata>,
    ) -> Result<(), PoolError>;

    /// Claims outstanding emissions for the caller for the given reserve's
    ///
    /// ### Arguments
    /// * `reserve_token_ids` - Vector of reserve token ids
    /// * `to` - The Identifier to send the claimed tokens to
    fn claim(e: Env, reserve_token_ids: Vec<u32>, to: Identifier) -> Result<(), PoolError>;

    /***** Reserve Emission Functions *****/

    /// Fetch the emission details for a given reserve token
    ///
    /// ### Arguments
    /// * `asset` - The contract address of the asset backing the reserve
    /// * `token_type` - The type of reserve token (0 for dToken / 1 for bToken)
    fn res_emis(
        e: Env,
        asset: BytesN<32>,
        token_type: u32,
    ) -> Result<Option<(ReserveEmissionsConfig, ReserveEmissionsData)>, PoolError>;
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
        storage.set_pool_status(1);
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
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 1_000_000_000,
            d_supply: 0,
            b_supply: 0,
            last_block: e.ledger().sequence(),
        };
        storage.set_res_data(asset, init_data);
    }

    fn config(e: Env, user: Identifier) -> u128 {
        let storage = StorageManager::new(&e);
        storage.get_user_config(user)
    }

    fn supply(e: Env, asset: BytesN<32>, amount: u64) -> Result<u64, PoolError> {
        let storage = StorageManager::new(&e);
        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);

        if storage.get_pool_status() == 2 {
            return Err(PoolError::InvalidPoolStatus);
        }

        let mut reserve = Reserve::load(&e, asset.clone());
        reserve.pre_action(&e, 1, invoker_id.clone())?;

        let to_mint = reserve.to_b_token(&amount);
        TokenClient::new(&e, asset).xfer_from(
            &Signature::Invoker,
            &0,
            &invoker_id,
            &get_contract_id(&e),
            &(amount as i128),
        );

        TokenClient::new(&e, reserve.config.b_token.clone()).mint(
            &Signature::Invoker,
            &0,
            &invoker_id,
            &(to_mint as i128),
        );

        let mut user_config = ReserveUsage::new(storage.get_user_config(invoker_id.clone()));
        if !user_config.is_supply(reserve.config.index) {
            user_config.set_supply(reserve.config.index, true);
            storage.set_user_config(invoker_id, user_config.config);
        }

        reserve.add_supply(&to_mint);
        reserve.set_data(&e);
        Ok(to_mint as u64)
    }

    fn withdraw(e: Env, asset: BytesN<32>, amount: u64, to: Identifier) -> Result<u64, PoolError> {
        let storage = StorageManager::new(&e);
        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);

        let mut reserve = Reserve::load(&e, asset.clone());
        reserve.pre_action(&e, 1, invoker_id.clone())?;

        let to_burn: u64;
        let to_return: u64;
        let b_token_client = TokenClient::new(&e, reserve.config.b_token.clone());
        if amount == u64::MAX {
            // if they input u64::MAX as the burn amount, burn 100% of their holdings
            to_burn = b_token_client.balance(&invoker_id) as u64;
            to_return = reserve.to_asset_from_b_token(&to_burn);
        } else {
            to_burn = reserve.to_b_token(&amount);
            to_return = amount;
        }

        let user_action = UserAction {
            asset: asset.clone(),
            b_token_delta: -(to_burn as i64),
            d_token_delta: 0,
        };
        let is_healthy = validate_hf(&e, &invoker_id, &user_action);
        if !is_healthy {
            return Err(PoolError::InvalidHf);
        }

        b_token_client.clawback(&Signature::Invoker, &0, &invoker_id, &(to_burn as i128));

        TokenClient::new(&e, asset).xfer(&Signature::Invoker, &0, &to, &(to_return as i128));

        let mut user_config = ReserveUsage::new(storage.get_user_config(invoker_id.clone()));
        if b_token_client.balance(&invoker_id) == 0 {
            user_config.set_supply(reserve.config.index, false);
            storage.set_user_config(invoker_id, user_config.config);
        }

        reserve.remove_supply(&to_burn);
        reserve.set_data(&e);
        Ok(to_burn)
    }

    fn borrow(e: Env, asset: BytesN<32>, amount: u64, to: Identifier) -> Result<u64, PoolError> {
        let storage = StorageManager::new(&e);
        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);

        if storage.get_pool_status() > 0 {
            return Err(PoolError::InvalidPoolStatus);
        }

        let mut reserve = Reserve::load(&e, asset.clone());
        reserve.pre_action(&e, 0, invoker_id.clone())?;

        let to_mint = reserve.to_d_token(&amount);
        let user_action = UserAction {
            asset: asset.clone(),
            b_token_delta: 0,
            d_token_delta: to_mint as i64,
        };
        let is_healthy = validate_hf(&e, &invoker_id, &user_action);
        if !is_healthy {
            return Err(PoolError::InvalidHf);
        }

        TokenClient::new(&e, reserve.config.d_token.clone()).mint(
            &Signature::Invoker,
            &0,
            &invoker_id,
            &(to_mint as i128),
        );

        TokenClient::new(&e, asset).xfer(&Signature::Invoker, &0, &to, &(amount as i128));

        let mut user_config = ReserveUsage::new(storage.get_user_config(invoker_id.clone()));
        if !user_config.is_liability(reserve.config.index) {
            user_config.set_liability(reserve.config.index, true);
            storage.set_user_config(invoker_id, user_config.config);
        }

        reserve.add_liability(&to_mint);
        reserve.set_data(&e);
        Ok(to_mint)
    }

    fn repay(
        e: Env,
        asset: BytesN<32>,
        amount: u64,
        on_behalf_of: Identifier,
    ) -> Result<u64, PoolError> {
        let storage = StorageManager::new(&e);
        let invoker = e.invoker();
        let invoker_id = Identifier::from(invoker);

        let mut reserve = Reserve::load(&e, asset.clone());
        reserve.pre_action(&e, 0, invoker_id.clone())?;

        let to_burn: u64;
        let to_repay: u64;
        let d_token_client = TokenClient::new(&e, reserve.config.d_token.clone());
        if amount == u64::MAX {
            // if they input u64::MAX as the repay amount, burn 100% of their holdings
            to_burn = d_token_client.balance(&invoker_id) as u64;
            to_repay = reserve.to_asset_from_d_token(&to_burn);
        } else {
            to_burn = reserve.to_d_token(&amount);
            to_repay = amount;
        }

        d_token_client.clawback(&Signature::Invoker, &0, &on_behalf_of, &(to_burn as i128));

        TokenClient::new(&e, asset).xfer_from(
            &Signature::Invoker,
            &0,
            &invoker_id,
            &get_contract_id(&e),
            &(to_repay as i128),
        );

        let mut user_config = ReserveUsage::new(storage.get_user_config(invoker_id.clone()));
        if d_token_client.balance(&invoker_id) == 0 {
            user_config.set_liability(reserve.config.index, false);
            storage.set_user_config(invoker_id, user_config.config);
        }

        reserve.remove_liability(&to_burn);
        reserve.set_data(&e);
        Ok(to_burn)
    }

    fn set_status(e: Env, pool_status: u32) -> Result<(), PoolError> {
        let storage = StorageManager::new(&e);
        let invoker = e.invoker();
        let invoker_id;
        match invoker {
            Address::Account(account_id) => invoker_id = Identifier::Account(account_id),
            Address::Contract(bytes) => invoker_id = Identifier::Ed25519(bytes),
        }

        if invoker_id != storage.get_admin() {
            return Err(PoolError::NotAuthorized);
        }

        storage.set_pool_status(pool_status);
        Ok(())
    }

    fn status(e: Env) -> u32 {
        let storage = StorageManager::new(&e);
        storage.get_pool_status()
    }

    /********** Emissions **********/

    fn get_emis(e: Env) -> Map<u32, u64> {
        let storage = StorageManager::new(&e);
        storage.get_pool_emissions()
    }

    fn updt_emis(e: Env) -> Result<u64, PoolError> {
        let bkstp_addr = EmitterClient::new(&e, BytesN::from_array(&e, &EMITTER)).get_bstop();
        let backstop = BackstopClient::new(&e, &bkstp_addr);
        let next_exp = backstop.next_dist();
        let pool_eps = backstop.pool_eps(&e.current_contract());
        emissions_manager::update_emissions(&e, next_exp, pool_eps)
    }

    fn set_emis(
        e: Env,
        res_emission_metadata: Vec<ReserveEmissionMetadata>,
    ) -> Result<(), PoolError> {
        let storage = StorageManager::new(&e);
        if Identifier::from(e.invoker()) != storage.get_admin() {
            return Err(PoolError::NotAuthorized);
        }

        emissions_manager::set_pool_emissions(&e, res_emission_metadata)
    }

    fn claim(e: Env, reserve_token_ids: Vec<u32>, to: Identifier) -> Result<(), PoolError> {
        let user = Identifier::from(e.invoker());
        let to_claim = emissions_distributor::calc_claim(&e, user, reserve_token_ids)?;

        if to_claim > 0 {
            let bkstp_addr = EmitterClient::new(&e, BytesN::from_array(&e, &EMITTER)).get_bstop();
            let backstop = BackstopClient::new(&e, &bkstp_addr);
            backstop.claim(&to, &to_claim);
        }

        Ok(())
    }

    /***** Reserve Emission Functions *****/

    fn res_emis(
        e: Env,
        asset: BytesN<32>,
        token_type: u32,
    ) -> Result<Option<(ReserveEmissionsConfig, ReserveEmissionsData)>, PoolError> {
        if token_type > 1 {
            return Err(PoolError::BadRequest);
        }

        let storage = StorageManager::new(&e);
        let res_list = storage.get_res_list();
        if let Some(res_index) = res_list.first_index_of(asset) {
            let res_token_index = res_index * 3 + token_type;
            if storage.has_res_emis_data(res_token_index) {
                return Ok(Some((
                    storage.get_res_emis_config(res_token_index).unwrap(),
                    storage.get_res_emis_data(res_token_index).unwrap(),
                )));
            }
            return Ok(None);
        }

        Err(PoolError::BadRequest)
    }
}

// ****** Helpers *****

fn get_contract_id(e: &Env) -> Identifier {
    Identifier::Contract(e.current_contract())
}
