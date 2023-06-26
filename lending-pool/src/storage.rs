use soroban_sdk::{
    contracttype, map, unwrap::UnwrapOptimized, vec, Address, Env, Map, Symbol, Vec,
};

use crate::{auctions::AuctionData, pool::Positions};

/********** Storage Types **********/

/// An action a user can take against the pool
#[derive(Clone)]
#[contracttype]
pub struct Action {
    pub action_type: u32, // 0 = supply, 1 = collateral deposit, 2 = withdrawal, 3 = borrow, 4 = repay
    pub reserve_index: u32,
    pub amount: i128,
}

/// The pool's config
#[derive(Clone)]
#[contracttype]
pub struct PoolConfig {
    pub oracle: Address,
    pub bstop_rate: u64, // the rate the backstop takes on accrued debt interest, expressed in 9 decimals
    pub status: u32,
}

/// The pool's emission config
#[derive(Clone)]
#[contracttype]
pub struct PoolEmissionConfig {
    pub config: u128,
    pub last_time: u64,
}

/// The configuration information about a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveConfig {
    pub index: u32,      // the index of the reserve in the list
    pub decimals: u32,   // the decimals used in both the bToken and underlying contract
    pub c_factor: u32,   // the collateral factor for the reserve scaled expressed in 7 decimals
    pub l_factor: u32,   // the liability factor for the reserve scaled expressed in 7 decimals
    pub util: u32,       // the target utilization rate scaled expressed in 7 decimals
    pub max_util: u32,   // the maximum allowed utilization rate scaled expressed in 7 decimals
    pub r_one: u32,      // the R1 value in the interest rate formula scaled expressed in 7 decimals
    pub r_two: u32,      // the R2 value in the interest rate formula scaled expressed in 7 decimals
    pub r_three: u32,    // the R3 value in the interest rate formula scaled expressed in 7 decimals
    pub reactivity: u32, // the reactivity constant for the reserve scaled expressed in 9 decimals
}

/// The data for a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveData {
    pub d_rate: i128, // the conversion rate from dToken to underlying expressed in 9 decimals
    pub b_rate: i128, // the conversion rate from bToken to underlying expressed with the underlying's decimals
    pub ir_mod: i128, // the interest rate curve modifier
    pub b_supply: i128, // the total supply of b tokens
    pub d_supply: i128, // the total supply of d tokens
    pub backstop_credit: i128, // the amount of underlying tokens currently owed to the backstop
    pub last_time: u64, // the last block the data was updated
}

/// The configuration of emissions for the reserve b or d token
///
/// `@dev` If this is updated, ReserveEmissionsData MUST also be updated
#[derive(Clone)]
#[contracttype]
pub struct ReserveEmissionsConfig {
    pub expiration: u64,
    pub eps: u64,
}

/// The emission data for the reserve b or d token
#[derive(Clone)]
#[contracttype]
pub struct ReserveEmissionsData {
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
pub struct UserReserveKey {
    user: Address,
    reserve_id: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct AuctionKey {
    user: Address,  // the Address whose assets are involved in the auction
    auct_type: u32, // the type of auction taking place
}

// TODO: See if we can avoid publishing this
#[derive(Clone)]
#[contracttype]
pub enum PoolDataKey {
    // The address that can manage the pool
    Admin,
    // The name of the pool
    Name,
    // The backstop ID for the pool
    Backstop,
    // BLND token ID
    BLNDTkn,
    // USDC token ID
    USDCTkn,
    // The config of the pool
    PoolConfig,
    // A list of the next reserve emission allocation percentages
    PoolEmis,
    // The expiration time for the pool emissions
    EmisExp,
    // A map of underlying asset's contract address to reserve config
    ResConfig(Address),
    // A map of underlying asset's contract address to reserve data
    ResData(Address),
    // A list of reserve where index -> underlying asset's contract address
    // -> note: dropped reserves are still present
    ResList,
    // The reserve's emission config
    EmisConfig(u32),
    // The reserve's emission data
    EmisData(u32),
    // Map of positions in the pool for a user
    Positions(Address),
    // The emission information for a reserve asset for a user
    UserEmis(UserReserveKey),
    // The auction's data
    Auction(AuctionKey),
    // A list of auctions and their associated data
    AuctData(Address),
}

/********** Storage **********/

/********** User **********/

pub fn get_user_positions(e: &Env, user: &Address) -> Positions {
    let key = PoolDataKey::Positions(user.clone());
    e.storage()
        .get::<PoolDataKey, Positions>(&key)
        .unwrap_or(Ok(Positions::env_default(e)))
        .unwrap_optimized()
}

pub fn set_user_positions(e: &Env, user: &Address, positions: &Positions) {
    let key = PoolDataKey::Positions(user.clone());
    e.storage().set::<PoolDataKey, Positions>(&key, positions);
}

/********** Admin **********/

// Fetch the current admin Address
///
/// ### Panics
/// If the admin does not exist
pub fn get_admin(e: &Env) -> Address {
    e.storage()
        .get_unchecked(&PoolDataKey::Admin)
        .unwrap_optimized()
}

/// Set a new admin
///
/// ### Arguments
/// * `new_admin` - The Address for the admin
pub fn set_admin(e: &Env, new_admin: &Address) {
    e.storage()
        .set::<PoolDataKey, Address>(&PoolDataKey::Admin, new_admin);
}

/// Checks if an admin is set
pub fn has_admin(e: &Env) -> bool {
    e.storage().has(&PoolDataKey::Admin)
}

/********** Metadata **********/

/// Set a pool name
///
/// ### Arguments
/// * `name` - The Name of the pool
pub fn set_name(e: &Env, name: &Symbol) {
    e.storage()
        .set::<PoolDataKey, Symbol>(&PoolDataKey::Name, name);
}

/********** Backstop **********/

/// Fetch the backstop ID for the pool
///
/// ### Panics
/// If no backstop is set
pub fn get_backstop(e: &Env) -> Address {
    e.storage()
        .get_unchecked(&PoolDataKey::Backstop)
        .unwrap_optimized()
}

/// Set a new backstop ID
///
/// ### Arguments
/// * `backstop` - The address of the backstop
pub fn set_backstop(e: &Env, backstop: &Address) {
    e.storage()
        .set::<PoolDataKey, Address>(&PoolDataKey::Backstop, backstop);
}

/********** External Token Contracts **********/

/// Fetch the BLND token ID
pub fn get_blnd_token(e: &Env) -> Address {
    e.storage()
        .get_unchecked(&PoolDataKey::BLNDTkn)
        .unwrap_optimized()
}

/// Set a new BLND token ID
///
/// ### Arguments
/// * `blnd_token_id` - The ID of the BLND token
pub fn set_blnd_token(e: &Env, blnd_token_id: &Address) {
    e.storage()
        .set::<PoolDataKey, Address>(&PoolDataKey::BLNDTkn, blnd_token_id);
}

/// Fetch the USDC token ID
pub fn get_usdc_token(e: &Env) -> Address {
    e.storage()
        .get_unchecked(&PoolDataKey::USDCTkn)
        .unwrap_optimized()
}

/// Set a new USDC token ID
///
/// ### Arguments
/// * `usdc_token_id` - The ID of the USDC token
pub fn set_usdc_token(e: &Env, usdc_token_id: &Address) {
    e.storage()
        .set::<PoolDataKey, Address>(&PoolDataKey::USDCTkn, usdc_token_id);
}

/********** Pool Config **********/

/// Fetch the pool configuration
///
/// ### Panics
/// If the pool's config is not set
pub fn get_pool_config(e: &Env) -> PoolConfig {
    e.storage()
        .get_unchecked(&PoolDataKey::PoolConfig)
        .unwrap_optimized()
}

/// Set the pool configuration
///
/// ### Arguments
/// * `config` - The contract address of the oracle
pub fn set_pool_config(e: &Env, config: &PoolConfig) {
    let key = PoolDataKey::PoolConfig;
    e.storage().set::<PoolDataKey, PoolConfig>(&key, config);
}

/********** Reserve Config (ResConfig) **********/

/// Fetch the reserve data for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
///
/// ### Panics
/// If the reserve does not exist
pub fn get_res_config(e: &Env, asset: &Address) -> ReserveConfig {
    let key = PoolDataKey::ResConfig(asset.clone());
    e.storage()
        .get::<PoolDataKey, ReserveConfig>(&key)
        .unwrap_optimized()
        .unwrap_optimized()
}

/// Set the reserve configuration for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `config` - The reserve configuration for the asset
pub fn set_res_config(e: &Env, asset: &Address, config: &ReserveConfig) {
    let key = PoolDataKey::ResConfig(asset.clone());

    e.storage().set::<PoolDataKey, ReserveConfig>(&key, &config);
}

/// Checks if a reserve exists for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
pub fn has_res(e: &Env, asset: &Address) -> bool {
    let key = PoolDataKey::ResConfig(asset.clone());
    e.storage().has(&key)
}

/********** Reserve Data (ResData) **********/

/// Fetch the reserve data for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
///
/// ### Panics
/// If the reserve does not exist
pub fn get_res_data(e: &Env, asset: &Address) -> ReserveData {
    let key = PoolDataKey::ResData(asset.clone());
    e.storage()
        .get::<PoolDataKey, ReserveData>(&key)
        .unwrap_optimized()
        .unwrap_optimized()
}

/// Set the reserve data for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `data` - The reserve data for the asset
pub fn set_res_data(e: &Env, asset: &Address, data: &ReserveData) {
    let key = PoolDataKey::ResData(asset.clone());
    e.storage().set::<PoolDataKey, ReserveData>(&key, data);
}

/********** Reserve List (ResList) **********/

/// Fetch the list of reserves
pub fn get_res_list(e: &Env) -> Vec<Address> {
    e.storage()
        .get::<PoolDataKey, Vec<Address>>(&PoolDataKey::ResList)
        .unwrap_or(Ok(vec![e])) // empty vec if nothing exists
        .unwrap_optimized()
}

/// Add a reserve to the back of the list and returns the index
///
/// ### Arguments
/// * `asset` - The contract address of the underlying asset
///
/// ### Panics
/// If the number of reserves in the list exceeds 32
///
// @dev: Once added it can't be removed
pub fn push_res_list(e: &Env, asset: &Address) -> u32 {
    let mut res_list = get_res_list(e);
    if res_list.len() == 32 {
        panic!("too many reserves")
    }
    res_list.push_back(asset.clone());
    let new_index = res_list.len() - 1;
    e.storage()
        .set::<PoolDataKey, Vec<Address>>(&PoolDataKey::ResList, &res_list);
    new_index
}

/********** Reserve Emissions **********/

/// Fetch the emission config for the reserve b or d token
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
pub fn get_res_emis_config(e: &Env, res_token_index: &u32) -> Option<ReserveEmissionsConfig> {
    let key = PoolDataKey::EmisConfig(res_token_index.clone());
    let result = e.storage().get::<PoolDataKey, ReserveEmissionsConfig>(&key);
    match result {
        Some(data) => Some(data.unwrap_optimized()),
        None => None,
    }
}

/// Set the emission config for the reserve b or d token
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
/// * `res_emis_config` - The new emission config for the reserve token
pub fn set_res_emis_config(
    e: &Env,
    res_token_index: &u32,
    res_emis_config: &ReserveEmissionsConfig,
) {
    let key = PoolDataKey::EmisConfig(res_token_index.clone());
    e.storage()
        .set::<PoolDataKey, ReserveEmissionsConfig>(&key, res_emis_config);
}

/// Fetch the emission data for the reserve b or d token
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
pub fn get_res_emis_data(e: &Env, res_token_index: &u32) -> Option<ReserveEmissionsData> {
    let key = PoolDataKey::EmisData(res_token_index.clone());
    let result = e.storage().get::<PoolDataKey, ReserveEmissionsData>(&key);
    match result {
        Some(data) => Some(data.unwrap_optimized()),
        None => None,
    }
}

/// Checks if the reserve token has emissions data
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
pub fn has_res_emis_data(e: &Env, res_token_index: &u32) -> bool {
    let key = PoolDataKey::EmisData(res_token_index.clone());
    e.storage().has(&key)
}

/// Set the emission data for the reserve b or d token
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
/// * `res_emis_data` - The new emission data for the reserve token
pub fn set_res_emis_data(e: &Env, res_token_index: &u32, res_emis_data: &ReserveEmissionsData) {
    let key = PoolDataKey::EmisData(res_token_index.clone());
    e.storage()
        .set::<PoolDataKey, ReserveEmissionsData>(&key, res_emis_data);
}

/********** User Emissions **********/

/// Fetch the users emission data for a reserve's b or d token
///
/// ### Arguments
/// * `user` - The address of the user
/// * `res_token_index` - The d/bToken index for the reserve
pub fn get_user_emissions(
    e: &Env,
    user: &Address,
    res_token_index: &u32,
) -> Option<UserEmissionData> {
    let key = PoolDataKey::UserEmis(UserReserveKey {
        user: user.clone(),
        reserve_id: res_token_index.clone(),
    });
    let result = e.storage().get::<PoolDataKey, UserEmissionData>(&key);
    match result {
        Some(data) => Some(data.unwrap_optimized()),
        None => None,
    }
}

/// Set the users emission data for a reserve's d or d token
///
/// ### Arguments
/// * `user` - The address of the user
/// * `res_token_index` - The d/bToken index for the reserve
/// * `data` - The new user emission d ata for the d/bToken
pub fn set_user_emissions(e: &Env, user: &Address, res_token_index: &u32, data: &UserEmissionData) {
    let key = PoolDataKey::UserEmis(UserReserveKey {
        user: user.clone(),
        reserve_id: res_token_index.clone(),
    });
    e.storage().set::<PoolDataKey, UserEmissionData>(&key, data);
}

/********** Pool Emissions **********/

/// Fetch the pool reserve emissions
pub fn get_pool_emissions(e: &Env) -> Map<u32, u64> {
    let key = PoolDataKey::PoolEmis;
    e.storage()
        .get::<PoolDataKey, Map<u32, u64>>(&key)
        .unwrap_or(Ok(map![e]))
        .unwrap_optimized()
}

/// Set the pool reserve emissions
///
/// ### Arguments
/// * `emissions` - The map of emissions by reserve token id to EPS
pub fn set_pool_emissions(e: &Env, emissions: &Map<u32, u64>) {
    let key = PoolDataKey::PoolEmis;
    e.storage()
        .set::<PoolDataKey, Map<u32, u64>>(&key, emissions);
}

/// Fetch the pool emission expiration timestamps
pub fn get_pool_emissions_expiration(e: &Env) -> u64 {
    let key = PoolDataKey::EmisExp;
    e.storage()
        .get::<PoolDataKey, u64>(&key)
        .unwrap_or(Ok(0))
        .unwrap_optimized()
}

/// Set the pool emission configuration
///
/// ### Arguments
/// * `expiration` - The pool's emission configuration
pub fn set_pool_emissions_expiration(e: &Env, expiration: &u64) {
    let key = PoolDataKey::EmisExp;
    e.storage().set::<PoolDataKey, u64>(&key, expiration);
}

/********** Auctions ***********/

/// Fetch the auction data for an auction
///
/// ### Arguments
/// * `auction_type` - The type of auction
/// * `user` - The user who is auctioning off assets
///
/// ### Panics
/// If the auction does not exist
pub fn get_auction(e: &Env, auction_type: &u32, user: &Address) -> AuctionData {
    let key = PoolDataKey::Auction(AuctionKey {
        user: user.clone(),
        auct_type: auction_type.clone(),
    });
    e.storage()
        .get::<PoolDataKey, AuctionData>(&key)
        .unwrap_optimized()
        .unwrap_optimized()
}

/// Check if an auction exists for the given type and user
///
/// ### Arguments
/// * `auction_type` - The type of auction
/// * `user` - The user who is auctioning off assets
pub fn has_auction(e: &Env, auction_type: &u32, user: &Address) -> bool {
    let key = PoolDataKey::Auction(AuctionKey {
        user: user.clone(),
        auct_type: auction_type.clone(),
    });
    e.storage().has(&key)
}

/// Set the the starting block for an auction
///
/// ### Arguments
/// * `auction_type` - The type of auction
/// * `user` - The user who is auctioning off assets
/// * `auction_data` - The auction data
pub fn set_auction(e: &Env, auction_type: &u32, user: &Address, auction_data: &AuctionData) {
    let key = PoolDataKey::Auction(AuctionKey {
        user: user.clone(),
        auct_type: auction_type.clone(),
    });
    e.storage()
        .set::<PoolDataKey, AuctionData>(&key, auction_data)
}

/// Remove an auction
///
/// ### Arguments
/// * `auction_type` - The type of auction
/// * `user` - The user who is auctioning off assets
pub fn del_auction(e: &Env, auction_type: &u32, user: &Address) {
    let key = PoolDataKey::Auction(AuctionKey {
        user: user.clone(),
        auct_type: auction_type.clone(),
    });
    e.storage().remove(&key)
}
