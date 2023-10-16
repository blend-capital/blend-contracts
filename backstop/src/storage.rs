use soroban_sdk::{
    contracttype, unwrap::UnwrapOptimized, vec, Address, Env, Map, Symbol, TryFromVal, Val, Vec,
};

use crate::backstop::{PoolBalance, UserBalance};

pub(crate) const LEDGER_THRESHOLD_SHARED: u32 = 172800; // ~ 10 days
pub(crate) const LEDGER_BUMP_SHARED: u32 = 241920; // ~ 14 days

pub(crate) const LEDGER_THRESHOLD_USER: u32 = 518400; // TODO: Check on phase 1 max ledger entry bump
pub(crate) const LEDGER_BUMP_USER: u32 = 535670; // TODO: Check on phase 1 max ledger entry bump

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
    PoolUSDC(Address),
    NextEmis,
    RewardZone,
    PoolEPS(Address),
    BEmisCfg(Address),
    BEmisData(Address),
    UEmisData(PoolUserKey),
    DropList,
    LPTknVal,
}

/****************************
**         Storage         **
****************************/

/// Bump the instance rent for the contract
pub fn bump_instance(e: &Env) {
    e.storage()
        .instance()
        .bump(LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}

/// Fetch an entry in persistent storage that has a default value if it doesn't exist
fn get_persistent_default<V: TryFromVal<Env, Val>>(
    e: &Env,
    key: &BackstopDataKey,
    default: V,
    bump_threshold: u32,
    bump_amount: u32,
) -> V {
    if let Some(result) = e.storage().persistent().get::<BackstopDataKey, V>(key) {
        e.storage()
            .persistent()
            .bump(key, bump_threshold, bump_amount);
        result
    } else {
        default
    }
}

/********** External Contracts **********/

/// Fetch the pool factory id
pub fn get_pool_factory(e: &Env) -> Address {
    e.storage()
        .instance()
        .get::<Symbol, Address>(&Symbol::new(e, "PoolFact"))
        .unwrap_optimized()
}

/// Set the pool factory
///
/// ### Arguments
/// * `pool_factory_id` - The ID of the pool factory
pub fn set_pool_factory(e: &Env, pool_factory_id: &Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, "PoolFact"), pool_factory_id);
}

/// Fetch the BLND token id
pub fn get_blnd_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get::<Symbol, Address>(&Symbol::new(e, "BLNDTkn"))
        .unwrap_optimized()
}

/// Set the BLND token id
///
/// ### Arguments
/// * `blnd_token_id` - The ID of the new BLND token
pub fn set_blnd_token(e: &Env, blnd_token_id: &Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, "BLNDTkn"), blnd_token_id);
}

/// Fetch the USDC token id
pub fn get_usdc_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get::<Symbol, Address>(&Symbol::new(e, "USDCTkn"))
        .unwrap_optimized()
}

/// Set the USDC token id
///
/// ### Arguments
/// * `usdc_token_id` - The ID of the new USDC token
pub fn set_usdc_token(e: &Env, usdc_token_id: &Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, "USDCTkn"), usdc_token_id);
}

/// Fetch the backstop token id
pub fn get_backstop_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get::<Symbol, Address>(&Symbol::new(e, "BckstpTkn"))
        .unwrap_optimized()
}

/// Checks if a backstop token is set for the backstop
pub fn has_backstop_token(e: &Env) -> bool {
    e.storage().instance().has(&Symbol::new(e, "BckstpTkn"))
}

/// Set the backstop token id
///
/// ### Arguments
/// * `backstop_token_id` - The ID of the new backstop token
pub fn set_backstop_token(e: &Env, backstop_token_id: &Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, "BckstpTkn"), backstop_token_id);
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
    get_persistent_default(
        e,
        &key,
        UserBalance {
            shares: 0,
            q4w: vec![&e],
        },
        LEDGER_THRESHOLD_USER,
        LEDGER_BUMP_USER,
    )
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
        .persistent()
        .set::<BackstopDataKey, UserBalance>(&key, balance);
}

/********** Pool Balance **********/

/// Fetch the balances for a given pool
///
/// ### Arguments
/// * `pool` - The pool the deposit is associated with
pub fn get_pool_balance(e: &Env, pool: &Address) -> PoolBalance {
    let key = BackstopDataKey::PoolBalance(pool.clone());
    get_persistent_default(
        e,
        &key,
        PoolBalance {
            shares: 0,
            tokens: 0,
            q4w: 0,
        },
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    )
}

/// Set the balances for a pool
///
/// ### Arguments
/// * `pool` - The pool the deposit is associated with
/// * `balance` - The pool balances
pub fn set_pool_balance(e: &Env, pool: &Address, balance: &PoolBalance) {
    let key = BackstopDataKey::PoolBalance(pool.clone());
    e.storage()
        .persistent()
        .set::<BackstopDataKey, PoolBalance>(&key, balance);
    e.storage()
        .persistent()
        .bump(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}

/// Fetch the balances for a given pool
///
/// ### Arguments
/// * `pool` - The pool the deposit is associated with
pub fn get_pool_usdc(e: &Env, pool: &Address) -> i128 {
    let key = BackstopDataKey::PoolUSDC(pool.clone());
    get_persistent_default(e, &key, 0i128, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED)
}

/// Set the balances for a pool
///
/// ### Arguments
/// * `pool` - The pool the deposit is associated with
/// * `balance` - The pool balances
pub fn set_pool_usdc(e: &Env, pool: &Address, balance: &i128) {
    let key = BackstopDataKey::PoolUSDC(pool.clone());
    e.storage()
        .persistent()
        .set::<BackstopDataKey, i128>(&key, balance);
    e.storage()
        .persistent()
        .bump(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}

/********** Distribution / Reward Zone **********/

/// Get the timestamp of when the next emission cycle begins
pub fn get_next_emission_cycle(e: &Env) -> u64 {
    get_persistent_default(
        e,
        &BackstopDataKey::NextEmis,
        0u64,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    )
}

/// Set the timestamp of when the next emission cycle begins
///
/// ### Arguments
/// * `timestamp` - The timestamp the distribution window will open
pub fn set_next_emission_cycle(e: &Env, timestamp: &u64) {
    e.storage()
        .persistent()
        .set::<BackstopDataKey, u64>(&BackstopDataKey::NextEmis, timestamp);
    e.storage().persistent().bump(
        &BackstopDataKey::NextEmis,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
}

/// Get the current pool addresses that are in the reward zone
///
// @dev - TODO: Once data access costs are available, find the breakeven point for splitting this up
pub fn get_reward_zone(e: &Env) -> Vec<Address> {
    get_persistent_default(
        e,
        &BackstopDataKey::RewardZone,
        vec![e],
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    )
}

/// Set the reward zone
///
/// ### Arguments
/// * `reward_zone` - The vector of pool addresses that comprise the reward zone
pub fn set_reward_zone(e: &Env, reward_zone: &Vec<Address>) {
    e.storage()
        .persistent()
        .set::<BackstopDataKey, Vec<Address>>(&BackstopDataKey::RewardZone, reward_zone);
    e.storage().persistent().bump(
        &BackstopDataKey::RewardZone,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
}

/// Get current emissions EPS the backstop is distributing to the pool
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_pool_eps(e: &Env, pool: &Address) -> i128 {
    let key = BackstopDataKey::PoolEPS(pool.clone());
    get_persistent_default(e, &key, 0i128, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED)
}

/// Set the current emissions EPS the backstop is distributing to the pool
///
/// ### Arguments
/// * `pool` - The pool
/// * `eps` - The eps being distributed to the pool
pub fn set_pool_eps(e: &Env, pool: &Address, eps: &i128) {
    let key = BackstopDataKey::PoolEPS(pool.clone());
    e.storage()
        .persistent()
        .set::<BackstopDataKey, i128>(&key, eps);
    e.storage()
        .persistent()
        .bump(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}

/********** Backstop Depositor Emissions **********/

/// Get the pool's backstop emissions config, or None
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_backstop_emis_config(e: &Env, pool: &Address) -> Option<BackstopEmissionConfig> {
    let key = BackstopDataKey::BEmisCfg(pool.clone());
    get_persistent_default::<Option<BackstopEmissionConfig>>(
        e,
        &key,
        None,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    )
}

/// Check if the pool's backstop emissions config is set
///
/// ### Arguments
/// * `pool` - The pool
pub fn has_backstop_emis_config(e: &Env, pool: &Address) -> bool {
    let key = BackstopDataKey::BEmisCfg(pool.clone());
    e.storage().persistent().has::<BackstopDataKey>(&key)
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
        .persistent()
        .set::<BackstopDataKey, BackstopEmissionConfig>(&key, backstop_emis_config);
}

/// Get the pool's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_backstop_emis_data(e: &Env, pool: &Address) -> Option<BackstopEmissionsData> {
    let key = BackstopDataKey::BEmisData(pool.clone());
    get_persistent_default::<Option<BackstopEmissionsData>>(
        e,
        &key,
        None,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    )
}

/// Set the pool's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool
/// * `backstop_emis_data` - The new emission data for the backstop
pub fn set_backstop_emis_data(e: &Env, pool: &Address, backstop_emis_data: &BackstopEmissionsData) {
    let key = BackstopDataKey::BEmisData(pool.clone());
    e.storage()
        .persistent()
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
    get_persistent_default::<Option<UserEmissionData>>(
        e,
        &key,
        None,
        LEDGER_THRESHOLD_USER,
        LEDGER_BUMP_USER,
    )
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
        .persistent()
        .set::<BackstopDataKey, UserEmissionData>(&key, user_emis_data);
}

/********** Drop Emissions **********/

/// Get the current pool addresses that are in the drop list and the amount of the initial distribution they receive
pub fn get_drop_list(e: &Env) -> Map<Address, i128> {
    e.storage()
        .temporary()
        .get::<BackstopDataKey, Map<Address, i128>>(&BackstopDataKey::DropList)
        .unwrap_optimized()
}

/// Set the drop list
///
/// ### Arguments
/// * `drop_list` - The map of pool addresses to the amount of the initial distribution they receive
pub fn set_drop_list(e: &Env, drop_list: &Map<Address, i128>) {
    e.storage()
        .temporary()
        .set::<BackstopDataKey, Map<Address, i128>>(&BackstopDataKey::DropList, drop_list);
    e.storage().temporary().bump(
        &BackstopDataKey::DropList,
        LEDGER_THRESHOLD_USER,
        LEDGER_BUMP_USER,
    );
}

/********** LP Token Value **********/

/// Get the last updated token value for the LP pool
pub fn get_lp_token_val(e: &Env) -> (i128, i128) {
    e.storage().persistent().bump(
        &BackstopDataKey::LPTknVal,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
    e.storage()
        .persistent()
        .get::<BackstopDataKey, (i128, i128)>(&BackstopDataKey::LPTknVal)
        .unwrap_optimized()
}

/// Set the reward zone
///
/// ### Arguments
/// * `share_val` - A tuple of (blnd_per_share, usdc_per_share)
pub fn set_lp_token_val(e: &Env, share_val: &(i128, i128)) {
    e.storage()
        .persistent()
        .set::<BackstopDataKey, (i128, i128)>(&BackstopDataKey::LPTknVal, share_val);
    e.storage().persistent().bump(
        &BackstopDataKey::LPTknVal,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
}
