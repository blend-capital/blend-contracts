use soroban_auth::Identifier;
use soroban_sdk::{contracttype, BytesN, Env, Vec, vec, Address};

use crate::types::{ReserveConfig, ReserveData};

#[derive(Clone)]
#[contracttype]
pub struct LiabilityKey {
    user: Address,
    asset: BytesN<32>
}

// TODO: See if we can avoid publishing this
#[derive(Clone)]
#[contracttype]
pub enum PoolDataKey {
    // The address that can manage the pool
    Admin,
    // The address of the oracle contract
    Oracle,
    // A map of underlying asset's contract address to reserve config
    ResConfig(BytesN<32>),
    // A map of underlying asset's contract address to reserve data
    ResData(BytesN<32>),
    // A list of reserve where index -> underlying asset's contract address
    // -> note: dropped reserves are still present
    ResList,
    // The configuration settings for a user
    UserConfig(Address),
}

// TODO: Consider reverting away from struct if mocking is not required
// #[cfg_attr(test, automock)]
pub trait PoolDataStore {
    /********** Admin **********/

    /// Fetch the current admin Identifier
    /// 
    /// ### Errors
    /// If the admin does not exist
    fn get_admin(&self) -> Identifier;

    /// Set a new admin
    /// 
    /// ### Arguments
    /// * `new_admin` - The Identifier for the admin
    fn set_admin(&self, new_admin: Identifier);

    /// Checks if an admin is set
    fn has_admin(&self) -> bool;

    /********** Oracle **********/

    /// Fetch the current oracle address
    /// 
    /// ### Errors
    /// If the oracle does not exist
    fn get_oracle(&self) -> BytesN<32>;

    /// Set a new oracle address
    /// 
    /// ### Arguments
    /// * `new_oracle` - The contract address of the oracle
    fn set_oracle(&self, new_oracle: BytesN<32>);

    /// Checks if an oracle is set
    fn has_oracle(&self) -> bool;

    /********** Reserve Config (ResConfig) **********/

    /// Fetch the reserve data for an asset
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// 
    /// ### Errors
    /// If the reserve does not exist
    fn get_res_config(&self, asset: BytesN<32>) -> ReserveConfig;

    /// Set the reserve configuration for an asset
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `config` - The reserve configuration for the asset
    fn set_res_config(&self, asset: BytesN<32>, config: ReserveConfig);

    /// Checks if a reserve exists for an asset
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    fn has_res(&self, asset: BytesN<32>) -> bool;

    /********** Reserve Data (ResData) **********/

    /// Fetch the reserve data for an asset
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// 
    /// ### Errors
    /// If the reserve does not exist
    fn get_res_data(&self, asset: BytesN<32>) -> ReserveData;

    /// Set the reserve data for an asset
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    /// * `data` - The reserve data for the asset
    fn set_res_data(&self, asset: BytesN<32>, data: ReserveData);

    /********** Reserve List (ResList) **********/

    /// Fetch the list of reserves
    fn get_res_list(&self) -> Vec<BytesN<32>>;

    /// Add a reserve to the back of the list and returns the index
    /// 
    /// ### Arguments
    /// * `asset` - The contract address of the underlying asset
    /// 
    /// ### Errors
    /// If the number of reserves in the list exceeds 32
    /// 
    // @dev: Once added it can't be removed
    fn push_res_list(&self, asset: BytesN<32>) -> u32;

    /********** UserConfig **********/

    /// Fetch the users reserve config
    /// 
    /// ### Arguments
    /// * `user` - The address of the user
    fn get_user_config(&self, user: Address) -> u64;

    /// Set the users reserve config
    /// 
    /// ### Arguments
    /// * `user` - The address of the user
    /// * `config` - The reserve config for the user
    fn set_user_config(&self, user: Address, config: u64);
}

pub struct StorageManager(Env);

impl PoolDataStore for StorageManager {
    /********** Admin **********/

    fn get_admin(&self) -> Identifier {
        self.0.data().get_unchecked(PoolDataKey::Admin).unwrap()
    }
    
    fn set_admin(&self, new_admin: Identifier) {
        self.env().data().set::<PoolDataKey, Identifier>(PoolDataKey::Admin, new_admin);
    }

    fn has_admin(&self) -> bool {
        self.env().data().has(PoolDataKey::Admin)
    }

    /********** Oracle **********/

    fn get_oracle(&self) -> BytesN<32> {
        self.env().data().get_unchecked(PoolDataKey::Oracle).unwrap()
    }

    fn set_oracle(&self, new_oracle: BytesN<32>) {
        self.env().data().set::<PoolDataKey, BytesN<32>>(PoolDataKey::Oracle, new_oracle);
    }

    fn has_oracle(&self) -> bool {
        self.env().data().has(PoolDataKey::Oracle)
    }

    /********** Reserve Config (ResConfig) **********/

    fn get_res_config(&self, asset: BytesN<32>) -> ReserveConfig {
        let key = PoolDataKey::ResConfig(asset);
        self.env().data().get::<PoolDataKey, ReserveConfig>(key)
            .unwrap()
            .unwrap()
    }

    fn set_res_config(&self, asset: BytesN<32>, config: ReserveConfig) {
        let key = PoolDataKey::ResConfig(asset.clone());
        let mut indexed_config = config.clone();

        // TODO: Might fit better in reserve module
        // add to reserve list if its new
        if !self.env().data().has(key.clone()) {
            let index = self.push_res_list(asset);
            indexed_config.index = index;
        }

        self.env().data().set::<PoolDataKey, ReserveConfig>(key, indexed_config);
    }

    fn has_res(&self, asset: BytesN<32>) -> bool {
        let key = PoolDataKey::ResConfig(asset);
        self.env().data().has(key)
    }

    /********** Reserve Data (ResData) **********/

    fn get_res_data(&self, asset: BytesN<32>) -> ReserveData {
        let key = PoolDataKey::ResData(asset);
        self.env().data().get::<PoolDataKey, ReserveData>(key)
            .unwrap()
            .unwrap()
    }

    fn set_res_data(&self, asset: BytesN<32>, data: ReserveData) {
        let key = PoolDataKey::ResData(asset);
        self.env().data().set::<PoolDataKey, ReserveData>(key, data);
    }

    /********** Reserve List (ResList) **********/

    fn get_res_list(&self) -> Vec<BytesN<32>> {
        self.env().data().get::<PoolDataKey, Vec<BytesN<32>>>(PoolDataKey::ResList)
            .unwrap_or(Ok(vec![&self.env()])) // empty vec if nothing exists
            .unwrap()
    }

    fn push_res_list(&self, asset: BytesN<32>) -> u32 {
        let mut res_list = self.get_res_list();
        if res_list.len() == 32 {
            panic!("too many reserves")
        }
        res_list.push_back(asset);
        let new_index = res_list.len() - 1;
        self.env().data().set::<PoolDataKey, Vec<BytesN<32>>>(PoolDataKey::ResList, res_list);
        new_index
    }

    /********** UserConfig **********/

    fn get_user_config(&self, user: Address) -> u64 {
        let key = PoolDataKey::UserConfig(user);
        self.env().data().get::<PoolDataKey, u64>(key)
            .unwrap_or(Ok(0))
            .unwrap()
    }

    fn set_user_config(&self, user: Address, config: u64) {
        let key = PoolDataKey::UserConfig(user);
        self.env().data().set::<PoolDataKey, u64>(key, config);
    }
}

impl StorageManager {
    #[inline(always)]
    pub(crate) fn env(&self) -> &Env {
        &self.0
    }

    #[inline(always)]
    pub(crate) fn new(env: &Env) -> StorageManager {
        StorageManager(env.clone())
    }
}
