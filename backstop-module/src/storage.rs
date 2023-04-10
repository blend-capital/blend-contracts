use soroban_sdk::{contracttype, vec, Address, BytesN, Env, Vec};

/********** Storage Types **********/

/// A deposit that is queued for withdrawal
#[derive(Clone)]
#[contracttype]
pub struct Q4W {
    pub amount: i128, // the amount of shares queued for withdrawal
    pub exp: u64,     // the expiration of the withdrawal
}

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
    pool: BytesN<32>,
    user: Address,
}

#[derive(Clone)]
#[contracttype]
pub enum BackstopDataKey {
    Shares(PoolUserKey),
    Q4W(PoolUserKey),
    PoolTkn(BytesN<32>),
    PoolShares(BytesN<32>),
    PoolQ4W(BytesN<32>),
    NextDist,
    RewardZone,
    PoolEPS(BytesN<32>),
    PoolEmis(BytesN<32>),
    BEmisCfg(BytesN<32>),
    BEmisData(BytesN<32>),
    UEmisData(PoolUserKey),
    BckstpTkn,
}

/****************************
**         Storage         **
****************************/

/********** Backstop Token **********/

/// Fetch the backstop token
pub fn get_backstop_token(e: &Env) -> BytesN<32> {
    e.storage()
        .get::<BackstopDataKey, BytesN<32>>(&BackstopDataKey::BckstpTkn)
        .unwrap()
        .unwrap()
}

/// Checks if a backstop token is set for the backstop
pub fn has_backstop_token(e: &Env) -> bool {
    e.storage().has(&BackstopDataKey::BckstpTkn)
}

/// Set the backstop token
///
/// ### Arguments
/// * `token_address` - The address of the new backstop token
pub fn set_backstop_token(e: &Env, token_address: &BytesN<32>) {
    e.storage()
        .set::<BackstopDataKey, BytesN<32>>(&BackstopDataKey::BckstpTkn, token_address);
}

/********** User Shares **********/

/// Fetch the balance of shares for a given pool for a user
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
/// * `user` - The owner of the deposit
pub fn get_shares(e: &Env, pool: &BytesN<32>, user: &Address) -> i128 {
    let key = BackstopDataKey::Shares(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    e.storage()
        .get::<BackstopDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set share balance for a user deposit in a pool
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
/// * `user` - The owner of the deposit
/// * `amount` - The amount of shares
pub fn set_shares(e: &Env, pool: &BytesN<32>, user: &Address, amount: &i128) {
    let key = BackstopDataKey::Shares(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    e.storage().set::<BackstopDataKey, i128>(&key, amount);
}

/********** User Queued for Withdrawal **********/

/// Fetch the current withdrawals the user has queued for a given pool
///
/// Returns an empty vec if no q4w's are present
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
/// * `user` - The owner of the deposit
pub fn get_q4w(e: &Env, pool: &BytesN<32>, user: &Address) -> Vec<Q4W> {
    let key = BackstopDataKey::Q4W(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    e.storage()
        .get::<BackstopDataKey, Vec<Q4W>>(&key)
        .unwrap_or(Ok(vec![e]))
        .unwrap()
}

/// Set the array of Q4W for a user's deposits in a pool
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
/// * `user` - The owner of the deposit
/// * `qw4` - The array of queued withdrawals
pub fn set_q4w(e: &Env, pool: &BytesN<32>, user: &Address, q4w: &Vec<Q4W>) {
    let key = BackstopDataKey::Q4W(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    e.storage().set::<BackstopDataKey, Vec<Q4W>>(&key, q4w);
}

/********** Pool Shares **********/

/// Fetch the total balance of shares for a given pool
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
pub fn get_pool_shares(e: &Env, pool: &BytesN<32>) -> i128 {
    let key = BackstopDataKey::PoolShares(pool.clone());
    e.storage()
        .get::<BackstopDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set share deposit total for a pool
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
/// * `amount` - The amount of shares
pub fn set_pool_shares(e: &Env, pool: &BytesN<32>, amount: &i128) {
    let key = BackstopDataKey::PoolShares(pool.clone());
    e.storage().set::<BackstopDataKey, i128>(&key, &amount);
}

/********** Pool Queued for Withdrawal **********/

/// Fetch the total balance of shares queued for withdraw for a given pool
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
pub fn get_pool_q4w(e: &Env, pool: &BytesN<32>) -> i128 {
    let key = BackstopDataKey::PoolQ4W(pool.clone());
    e.storage()
        .get::<BackstopDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set the total amount of shares queued for withdrawal for a pool
///
/// ### Arguments
/// * `pool` - The pool the backstop deposit represents
/// * `amount` - The amount of shares queued for withdrawal for the pool
pub fn set_pool_q4w(e: &Env, pool: &BytesN<32>, amount: &i128) {
    let key = BackstopDataKey::PoolQ4W(pool.clone());
    e.storage().set::<BackstopDataKey, i128>(&key, amount);
}

/********** Pool Tokens **********/

/// Get the balance of tokens in the backstop for a pool
///
/// ### Arguments
/// * `pool` - The pool the backstop balance belongs to
pub fn get_pool_tokens(e: &Env, pool: &BytesN<32>) -> i128 {
    let key = BackstopDataKey::PoolTkn(pool.clone());
    e.storage()
        .get::<BackstopDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set the balance of tokens in the backstop for a pool
///
/// ### Arguments
/// * `pool` - The pool the backstop balance belongs to
/// * `amount` - The amount of tokens attributed to the pool
pub fn set_pool_tokens(e: &Env, pool: &BytesN<32>, amount: &i128) {
    let key = BackstopDataKey::PoolTkn(pool.clone());
    e.storage().set::<BackstopDataKey, i128>(&key, amount);
}

/********** Distribution / Reward Zone **********/

/// Get the timestamp of when the next distribution window opens
pub fn get_next_dist(e: &Env) -> u64 {
    e.storage()
        .get::<BackstopDataKey, u64>(&BackstopDataKey::NextDist)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set the timestamp of when the next distribution window opens
///
/// ### Arguments
/// * `timestamp` - The timestamp the distribution window will open
pub fn set_next_dist(e: &Env, timestamp: &u64) {
    e.storage()
        .set::<BackstopDataKey, u64>(&BackstopDataKey::NextDist, timestamp);
}

/// Get the current pool addresses that are in the reward zone
///
// @dev - TODO: Once data access costs are available, find the breakeven point for splitting this up
pub fn get_reward_zone(e: &Env) -> Vec<BytesN<32>> {
    e.storage()
        .get::<BackstopDataKey, Vec<BytesN<32>>>(&BackstopDataKey::RewardZone)
        .unwrap_or(Ok(vec![&e]))
        .unwrap()
}

/// Set the reward zone
///
/// ### Arguments
/// * `reward_zone` - The vector of pool addresses that comprise the reward zone
pub fn set_reward_zone(e: &Env, reward_zone: &Vec<BytesN<32>>) {
    e.storage()
        .set::<BackstopDataKey, Vec<BytesN<32>>>(&BackstopDataKey::RewardZone, reward_zone);
}

/// Get current emissions EPS the backstop is distributing to the pool
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_pool_eps(e: &Env, pool: &BytesN<32>) -> i128 {
    let key = BackstopDataKey::PoolEPS(pool.clone());
    e.storage()
        .get::<BackstopDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set the current emissions EPS the backstop is distributing to the pool
///
/// ### Arguments
/// * `pool` - The pool
/// * `eps` - The eps being distributed to the pool
pub fn set_pool_eps(e: &Env, pool: &BytesN<32>, eps: &i128) {
    let key = BackstopDataKey::PoolEPS(pool.clone());
    e.storage().set::<BackstopDataKey, i128>(&key, eps);
}

/// Get current emissions allotment the backstop has distributed to the pool
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_pool_emis(e: &Env, pool: &BytesN<32>) -> i128 {
    let key = BackstopDataKey::PoolEmis(pool.clone());
    e.storage()
        .get::<BackstopDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set the current emissions allotment the backstop has distributed to the pool
///
/// ### Arguments
/// * `pool` - The pool
/// * `amount` - The pool's emission allotment
pub fn set_pool_emis(e: &Env, pool: &BytesN<32>, amount: &i128) {
    let key = BackstopDataKey::PoolEmis(pool.clone());
    e.storage().set::<BackstopDataKey, i128>(&key, amount);
}

/********** Backstop Depositor Emissions **********/

/// Get the pool's backstop emissions config, or None if
///
/// ### Arguments
/// * `pool` - The pool
pub fn get_backstop_emis_config(e: &Env, pool: &BytesN<32>) -> Option<BackstopEmissionConfig> {
    let key = BackstopDataKey::BEmisCfg(pool.clone());
    let result = e
        .storage()
        .get::<BackstopDataKey, BackstopEmissionConfig>(&key);
    match result {
        Some(data) => Some(data.unwrap()),
        None => None,
    }
}

/// Set the pool's backstop emissions config
///
/// ### Arguments
/// * `pool` - The pool
/// * `backstop_emis_config` - The new emission data for the backstop
pub fn set_backstop_emis_config(
    e: &Env,
    pool: &BytesN<32>,
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
pub fn get_backstop_emis_data(e: &Env, pool: &BytesN<32>) -> Option<BackstopEmissionsData> {
    let key = BackstopDataKey::BEmisData(pool.clone());
    let result = e
        .storage()
        .get::<BackstopDataKey, BackstopEmissionsData>(&key);
    match result {
        Some(data) => Some(data.unwrap()),
        None => None,
    }
}

/// Set the pool's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool
/// * `backstop_emis_data` - The new emission data for the backstop
pub fn set_backstop_emis_data(
    e: &Env,
    pool: &BytesN<32>,
    backstop_emis_data: &BackstopEmissionsData,
) {
    let key = BackstopDataKey::BEmisData(pool.clone());
    e.storage()
        .set::<BackstopDataKey, BackstopEmissionsData>(&key, backstop_emis_data);
}

/// Get the user's backstop emissions data
///
/// ### Arguments
/// * `pool` - The pool whose backstop the user's emissions are for
/// * `user` - The user's address
pub fn get_user_emis_data(e: &Env, pool: &BytesN<32>, user: &Address) -> Option<UserEmissionData> {
    let key = BackstopDataKey::UEmisData(PoolUserKey {
        pool: pool.clone(),
        user: user.clone(),
    });
    let result = e.storage().get::<BackstopDataKey, UserEmissionData>(&key);
    match result {
        Some(data) => Some(data.unwrap()),
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
    pool: &BytesN<32>,
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
