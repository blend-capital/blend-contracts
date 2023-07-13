use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, vec, Address, Env, Vec};

use crate::backstop::{PoolBalance, UserBalance};

/********** Storage Types **********/

// The emission configuration for a pool's backstop
#[derive(Clone)]
#[contracttype]
pub struct BackstopEmissionConfig {
    pub expiration: u64,
    pub eps: u64,
}

// The emission data for a pool's backstop
#[derive(Clone)]
#[contracttype]
pub struct BackstopEmissionsData {
    pub index: i128,
    pub last_time: u64,
}

/// The user emission data for the reserve b or d token
#[derive(Clone)]
#[contracttype]
pub struct UserEmissionData {
    pub index: i128,
    pub accrued: i128,
}

/********** Storage Key Types **********/

#[derive(Clone)]
#[contracttype]
pub struct PoolUserKey {
    pool: Address,
    user: Address,
}

#[derive(Clone)]
#[contracttype]
pub enum BackstopDataKey {
    UserBalance(PoolUserKey),
    PoolBalance(Address),
    NextEmis,
    RewardZone,
    PoolEPS(Address),
    BEmisCfg(Address),
    BEmisData(Address),
    UEmisData(PoolUserKey),
    BckstpTkn,
    PoolFact,
    BLNDTkn,
}

/****************************
**         Storage         **
****************************/

/********** External Contracts **********/

/// Fetch the pool factory id
pub fn get_pool_factory(e: &Env) -> Address {
    e.storage()
        .get::<BackstopDataKey, Address>(&BackstopDataKey::PoolFact)
        .unwrap_optimized()
        .unwrap_optimized()
}

/// Set the pool factory
///
/// ### Arguments
/// * `pool_factory_id` - The ID of the pool factory
pub fn set_pool_factory(e: &Env, pool_factory_id: &Address) {
    e.storage()
        .set::<BackstopDataKey, Address>(&BackstopDataKey::PoolFact, pool_factory_id);
}

/// Fetch the BLND token id
pub fn get_blnd_token(e: &Env) -> Address {
    e.storage()
        .get::<BackstopDataKey, Address>(&BackstopDataKey::BLNDTkn)
        .unwrap_optimized()
        .unwrap_optimized()
}

/// Set the BLND token id
///
/// ### Arguments
/// * `blnd_token_id` - The ID of the new BLND token
pub fn set_blnd_token(e: &Env, blnd_token_id: &Address) {
    e.storage()
        .set::<BackstopDataKey, Address>(&BackstopDataKey::BLNDTkn, blnd_token_id);
}

/// Fetch the backstop token id
pub fn get_backstop_token(e: &Env) -> Address {
    e.storage()
        .get::<BackstopDataKey, Address>(&BackstopDataKey::BckstpTkn)
        .unwrap_optimized()
        .unwrap_optimized()
}

/// Checks if a backstop token is set for the backstop
pub fn has_backstop_token(e: &Env) -> bool {
    e.storage().has(&BackstopDataKey::BckstpTkn)
}

/// Set the backstop token id
///
/// ### Arguments
/// * `backstop_token_id` - The ID of the new backstop token
pub fn set_backstop_token(e: &Env, backstop_token_id: &Address) {
    e.storage()
        .set::<BackstopDataKey, Address>(&BackstopDataKey::BckstpTkn, backstop_token_id);
}

/********** User Shares **********/

/// Fetch the balance's for a given user
///
/// ### Arguments
/// * `pool` - The pool the balance is associated with
/// * `user` - The owner of the deposit
pub fn get_user_balance(e: &Env, pool: &Address, user: &Address) -> UserBalance {
    let key = BackstopDataKey::UserBalance(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    e.storage()
        .get::<BackstopDataKey, UserBalance>(&key)
        .unwrap_or(Ok(UserBalance {
            shares: 0,
            q4w: vec![e],
        }))
        .unwrap_optimized()
}

/// Set share balance for a user deposit in a pool
///
/// ### Arguments
/// * `pool` - The pool the balance is associated with
/// * `user` - The owner of the deposit
/// * `balance` - The user balance
pub fn set_user_balance(e: &Env, pool: &Address, user: &Address, balance: &UserBalance) {
    let key = BackstopDataKey::UserBalance(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    e.storage()
        .set::<BackstopDataKey, UserBalance>(&key, balance);
}

/********** Pool Balance **********/

/// Fetch the balances for a given pool
///
/// ### Arguments
/// * `pool` - The pool the deposit is associated with
pub fn get_pool_balance(e: &Env, pool: &Address) -> PoolBalance {
    let key = BackstopDataKey::PoolBalance(pool.clone());
    e.storage()
        .get::<BackstopDataKey, PoolBalance>(&key)
        .unwrap_or(Ok(PoolBalance {
            shares: 0,
            tokens: 0,
            q4w: 0,
        }))
        .unwrap_optimized()
}

/// Set the balances for a pool
///
/// ### Arguments
/// * `pool` - The pool the deposit is associated with
/// * `balance` - The pool balances
pub fn set_pool_balance(e: &Env, pool: &Address, balance: &PoolBalance) {
    let key = BackstopDataKey::PoolBalance(pool.clone());
    e.storage()
        .set::<BackstopDataKey, PoolBalance>(&key, &balance);
}

/********** Distribution / Reward Zone **********/

/// Get the timestamp of when the next emission cycle begins
pub fn get_next_emission_cycle(e: &Env) -> u64 {
    e.storage()
        .get::<BackstopDataKey, u64>(&BackstopDataKey::NextEmis)
        .unwrap_or(Ok(0))
        .unwrap_optimized()
}

/// Set the timestamp of when the next emission cycle begins
///
/// ### Arguments
/// * `timestamp` - The timestamp the distribution window will open
pub fn set_next_emission_cycle(e: &Env, timestamp: &u64) {
    e.storage()
        .set::<BackstopDataKey, u64>(&BackstopDataKey::NextEmis, timestamp);
}

/// Get the current pool addresses that are in the reward zone
///
// @dev - TODO: Once data access costs are available, find the breakeven point for splitting this up
pub fn get_reward_zone(e: &Env) -> Vec<Address> {
    e.storage()
        .get::<BackstopDataKey, Vec<Address>>(&BackstopDataKey::RewardZone)
        .unwrap_or(Ok(vec![&e]))
        .unwrap_optimized()
}

/// Set the reward zone
///
/// ### Arguments
/// * `reward_zone` - The vector of pool addresses that comprise the reward zone
pub fn set_reward_zone(e: &Env, reward_zone: &Vec<Address>) {
    e.storage()
        .set::<BackstopDataKey, Vec<Address>>(&BackstopDataKey::RewardZone, reward_zone);
}

/// Get current emissions EPS the backstop is distributing to the pool
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_pool_eps(e: &Env, pool: &Address) -> i128 {
    let key = BackstopDataKey::PoolEPS(pool.clone());
    e.storage()
        .get::<BackstopDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap_optimized()
}

/// Set the current emissions EPS the backstop is distributing to the pool
///
/// ### Arguments
/// * `pool` - The pool
/// * `eps` - The eps being distributed to the pool
pub fn set_pool_eps(e: &Env, pool: &Address, eps: &i128) {
    let key = BackstopDataKey::PoolEPS(pool.clone());
    e.storage().set::<BackstopDataKey, i128>(&key, eps);
}

/********** Backstop Depositor Emissions **********/

/// Get the pool's backstop emissions config, or None
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_backstop_emis_config(e: &Env, pool: &Address) -> Option<BackstopEmissionConfig> {
    let key = BackstopDataKey::BEmisCfg(pool.clone());
    let result = e
        .storage()
        .get::<BackstopDataKey, BackstopEmissionConfig>(&key);
    match result {
        Some(data) => Some(data.unwrap_optimized()),
        None => None,
    }
}

/// Check if the pool's backstop emissions config is set
///
/// ### Arguments
/// * `pool` - The pool
pub fn has_backstop_emis_config(e: &Env, pool: &Address) -> bool {
    let key = BackstopDataKey::BEmisCfg(pool.clone());
    e.storage().has::<BackstopDataKey>(&key)
}

/// Set the pool's backstop emissions config
///
/// ### Arguments
/// * `pool` - The pool
/// * `backstop_emis_config` - The new emission data for the backstop
pub fn set_backstop_emis_config(
    e: &Env,
    pool: &Address,
    backstop_emis_config: &BackstopEmissionConfig,
) {
    let key = BackstopDataKey::BEmisCfg(pool.clone());
    e.storage()
        .set::<BackstopDataKey, BackstopEmissionConfig>(&key, backstop_emis_config);
}

/// Get the pool's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_backstop_emis_data(e: &Env, pool: &Address) -> Option<BackstopEmissionsData> {
    let key = BackstopDataKey::BEmisData(pool.clone());
    let result = e
        .storage()
        .get::<BackstopDataKey, BackstopEmissionsData>(&key);
    match result {
        Some(data) => Some(data.unwrap_optimized()),
        None => None,
    }
}

/// Set the pool's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool
/// * `backstop_emis_data` - The new emission data for the backstop
pub fn set_backstop_emis_data(e: &Env, pool: &Address, backstop_emis_data: &BackstopEmissionsData) {
    let key = BackstopDataKey::BEmisData(pool.clone());
    e.storage()
        .set::<BackstopDataKey, BackstopEmissionsData>(&key, backstop_emis_data);
}

/// Get the user's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool whose backstop the user's emissions are for
/// * `user` - The user's address
pub fn get_user_emis_data(e: &Env, pool: &Address, user: &Address) -> Option<UserEmissionData> {
    let key = BackstopDataKey::UEmisData(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    let result = e.storage().get::<BackstopDataKey, UserEmissionData>(&key);
    match result {
        Some(data) => Some(data.unwrap_optimized()),
        None => None,
    }
}

/// Set the user's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool whose backstop the user's emissions are for
/// * `user` - The user's address
/// * `user_emis_data` - The new emission data for the user
pub fn set_user_emis_data(
    e: &Env,
    pool: &Address,
    user: &Address,
    user_emis_data: &UserEmissionData,
) {
    let key = BackstopDataKey::UEmisData(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    e.storage()
        .set::<BackstopDataKey, UserEmissionData>(&key, user_emis_data);
}
