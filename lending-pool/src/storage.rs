use soroban_auth::Identifier;
use soroban_sdk::{contracttype, BytesN, Env, Vec, vec, Address, BigInt};

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
    // The liability balance for a user
    // TODO: Revisit this as native token contract that disables transferability
    Liability(LiabilityKey)
}

/********** Admin **********/

/// Fetch the current admin Identifier
/// 
/// ### Errors
/// If the admin does not exist
pub fn get_admin(e: &Env) -> Identifier {
    e.data().get_unchecked(PoolDataKey::Admin).unwrap()
}

/// Set a new admin
/// 
/// ### Arguments
/// * `new_admin` - The Identifier for the admin
pub fn set_admin(e: &Env, new_admin: Identifier) {
    e.data().set::<PoolDataKey, Identifier>(PoolDataKey::Admin, new_admin);
}

/// Checks if an admin is set
pub fn has_admin(e: &Env) -> bool {
    e.data().has(PoolDataKey::Admin)
}

/********** Oracle **********/

/// Fetch the current oracle address
/// 
/// ### Errors
/// If the oracle does not exist
pub fn get_oracle(e: &Env) -> BytesN<32> {
    e.data().get_unchecked(PoolDataKey::Oracle).unwrap()
}

/// Set a new oracle address
/// 
/// ### Arguments
/// * `new_oracle` - The contract address of the oracle
pub fn set_oracle(e: &Env, new_oracle: BytesN<32>) {
    e.data().set::<PoolDataKey, BytesN<32>>(PoolDataKey::Oracle, new_oracle);
}

/// Checks if an oracle is set
pub fn has_oracle(e: &Env) -> bool {
    e.data().has(PoolDataKey::Oracle)
}

/********** Reserve Config (ResConfig) **********/

/// Fetch the reserve data for an asset
/// 
/// ### Arguments
/// * `asset` - The contract address of the asset
/// 
/// ### Errors
/// If the reserve does not exist
pub fn get_res_config(e: &Env, asset: BytesN<32>) -> ReserveConfig {
    let key = PoolDataKey::ResConfig(asset);
    e.data().get::<PoolDataKey, ReserveConfig>(key)
        .unwrap()
        .unwrap()
}

/// Set the reserve configuration for an asset
/// 
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `config` - The reserve configuration for the asset
pub fn set_res_config(e: &Env, asset: BytesN<32>, config: ReserveConfig) {
    let key = PoolDataKey::ResConfig(asset.clone());

    // add to reserve list if its new
    if !e.data().has(key.clone()) {
        add_res_to_list(e, asset);
    }

    e.data().set::<PoolDataKey, ReserveConfig>(key, config);
}

/// Checks if a reserve exists for an asset
/// 
/// ### Arguments
/// * `asset` - The contract address of the asset
pub fn has_res(e: &Env, asset: BytesN<32>) -> bool {
    let key = PoolDataKey::ResConfig(asset);
    e.data().has(key)
}

/********** Reserve Data (ResData) **********/

/// Fetch the reserve data for an asset
/// 
/// ### Arguments
/// * `asset` - The contract address of the asset
/// 
/// ### Errors
/// If the reserve does not exist
pub fn get_res_data(e: &Env, asset: BytesN<32>) -> ReserveData {
    let key = PoolDataKey::ResData(asset);
    e.data().get::<PoolDataKey, ReserveData>(key)
        .unwrap()
        .unwrap()
}

/// Set the reserve data for an asset
/// 
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `data` - The reserve data for the asset
pub fn set_res_data(e: &Env, asset: BytesN<32>, data: ReserveData) {
    let key = PoolDataKey::ResData(asset);
    e.data().set::<PoolDataKey, ReserveData>(key, data);
}

/********** Reserve List (ResList) **********/

/// Fetch the list of reserves
pub fn get_res_list(e: &Env) -> Vec<BytesN<32>> {
    e.data().get::<PoolDataKey, Vec<BytesN<32>>>(PoolDataKey::ResList)
        .unwrap_or(Ok(vec![e])) // empty vec if nothing exists
        .unwrap()
}

/// Add a reserve to the list
/// 
/// ### Arguments
/// * `asset` - The contract address of the underlying asset
/// 
/// ### Errors
/// If the number of reserves in the list exceeds 32
/// 
// @dev: Once added it can't be removed
fn add_res_to_list(e: &Env, asset: BytesN<32>) {
    let mut res_list = get_res_list(e);
    if res_list.len() == 32 {
        panic!("too many reserves")
    }
    res_list.push_back(asset);
    e.data().set::<PoolDataKey, Vec<BytesN<32>>>(PoolDataKey::ResList, res_list);
}

/********** UserConfig **********/

/// Fetch the users reserve config
/// 
/// ### Arguments
/// * `user` - The address of the user
pub fn get_user_config(e: &Env, user: Address) -> u64 {
    let key = PoolDataKey::UserConfig(user);
    e.data().get::<PoolDataKey, u64>(key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set the users reserve config
/// 
/// ### Arguments
/// * `user` - The address of the user
/// * `config` - The reserve config for the user
pub fn set_user_config(e: &Env, user: Address, config: u64) {
    let key = PoolDataKey::UserConfig(user);
    e.data().set::<PoolDataKey, u64>(key, config);
}

/********** Liability **********/

/// Fetch the users liability in dTokens
/// 
/// ### Arguments
/// * `user` - The address of the user
/// * `asset` - The contract address of the underlying asset
pub fn get_liability(e: &Env, user: Address, asset: BytesN<32>) -> BigInt {
    let key = PoolDataKey::Liability(LiabilityKey { user, asset });
    e.data().get::<PoolDataKey, BigInt>(key)
        .unwrap_or(Ok(BigInt::zero(e)))
        .unwrap()
}

/// Set the users liability
/// 
/// ### Arguments
/// * `user` - The address of the user
/// * `asset` - The contract address of the underlying asset
/// * `amount` - The liability size in dTokens
pub fn set_liability(e: &Env, user: Address, asset: BytesN<32>, amount: BigInt) {
    let key = PoolDataKey::Liability(LiabilityKey { user, asset });
    e.data().set::<PoolDataKey, BigInt>(key, amount);
}