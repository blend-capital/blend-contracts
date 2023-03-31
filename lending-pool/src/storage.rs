use soroban_sdk::{contracttype, vec, Address, BytesN, Env, Map, Vec};

use crate::auctions::AuctionData;

/********** Storage Types **********/

/// The pool's config
#[derive(Clone)]
#[contracttype]
pub struct PoolConfig {
    pub oracle: BytesN<32>,
    pub bstop_rate: u64,
    pub status: u32,
}

/// The pool's emission config
#[derive(Clone)]
#[contracttype]
pub struct PoolEmissionConfig {
    pub config: u128,
    pub last_time: u64,
}

/// The mutable configuration information about a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveMetadata {
    pub decimals: u32,   // the decimals used in both the bToken and underlying contract
    pub c_factor: u32,   // the collateral factor for the reserve
    pub l_factor: u32,   // the liability factor for the reserve
    pub util: u32,       // the target utilization rate
    pub max_util: u32,   // the maximum allowed utilization rate
    pub r_one: u32,      // the R1 value in the interest rate formula
    pub r_two: u32,      // the R2 value in the interest rate formula
    pub r_three: u32,    // the R3 value in the interest rate formula
    pub reactivity: u32, // the reactivity constant for the reserve
}

/// The configuration information about a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveConfig {
    pub b_token: BytesN<32>, // the address of the bToken contract
    pub d_token: BytesN<32>, // the address of the dToken contract
    pub index: u32,          // the index of the reserve in the list
    pub decimals: u32,       // the decimals used in both the bToken and underlying contract
    pub c_factor: u32,       // the collateral factor for the reserve
    pub l_factor: u32,       // the liability factor for the reserve
    pub util: u32,           // the target utilization rate
    pub max_util: u32,       // the maximum allowed utilization rate
    pub r_one: u32,          // the R1 value in the interest rate formula
    pub r_two: u32,          // the R2 value in the interest rate formula
    pub r_three: u32,        // the R3 value in the interest rate formula
    pub reactivity: u32,     // the reactivity constant for the reserve
}

/// The data for a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveData {
    // TODO: These rates are correlated and can be simplified if both the b/dTokens have a totalSupply
    pub d_rate: i128, // the conversion rate from dToken to underlying - NOTE: stored as 9 decimals
    pub ir_mod: i128, // the interest rate curve modifier
    // TODO: Remove or fix these once final choice on totalSupply for native or custom tokens added
    pub b_supply: i128, // the total supply of b tokens - TODO: File issue to support u128 (likely added on token update to u128)
    pub d_supply: i128, // the total supply of d tokens
    pub last_block: u32, // the last block the data was updated
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
    // The backstop address for the pool
    Backstop,
    // TODO: Remove after: https://github.com/stellar/rs-soroban-sdk/issues/868
    BkstpAddr,
    // Token Hashes
    TokenHash,
    // The config of the pool
    PoolConfig,
    // A list of the next reserve emission allocation percentages
    PoolEmis,
    // The reserve configuration for emissions
    PEConfig,
    // A map of underlying asset's contract address to reserve config
    ResConfig(BytesN<32>),
    // A map of underlying asset's contract address to reserve data
    ResData(BytesN<32>),
    // A list of reserve where index -> underlying asset's contract address
    // -> note: dropped reserves are still present
    ResList,
    // The reserve's emission config
    EmisConfig(u32),
    // The reserve's emission data
    EmisData(u32),
    // The configuration settings for a user
    UserConfig(Address),
    // The emission information for a reserve asset for a user
    UserEmis(UserReserveKey),
    // The auction's data
    Auction(AuctionKey),
    // A list of auctions and their associated data
    AuctData(Address),
}

/********** Storage **********/

/********** Admin **********/

// Fetch the current admin Address
///
/// ### Errors
/// If the admin does not exist
pub fn get_admin(e: &Env) -> Address {
    e.storage().get_unchecked(&PoolDataKey::Admin).unwrap()
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

/********** Backstop **********/

/// Fetch the backstop for the pool
///
/// ### Errors
/// If no backstop is set
pub fn get_backstop(e: &Env) -> BytesN<32> {
    e.storage().get_unchecked(&PoolDataKey::Backstop).unwrap()
}

/// Set a new admin
///
/// ### Arguments
/// * `backstop` - The address of the backstop
pub fn set_backstop(e: &Env, backstop: &BytesN<32>) {
    e.storage()
        .set::<PoolDataKey, BytesN<32>>(&PoolDataKey::Backstop, backstop);
}

/// TODO: Remove after: https://github.com/stellar/rs-soroban-sdk/issues/868
pub fn get_backstop_address(e: &Env) -> Address {
    e.storage().get_unchecked(&PoolDataKey::BkstpAddr).unwrap()
}

/// TODO: Remove after: https://github.com/stellar/rs-soroban-sdk/issues/868
pub fn set_backstop_address(e: &Env, backstop: &Address) {
    e.storage()
        .set::<PoolDataKey, Address>(&PoolDataKey::BkstpAddr, backstop);
}

/********** Token Hashes **********/

/// Fetch the B and D token hashes for the pool
///
/// ### Errors
/// If the pool has not been initialized
pub fn get_token_hashes(e: &Env) -> (BytesN<32>, BytesN<32>) {
    e.storage().get_unchecked(&PoolDataKey::TokenHash).unwrap()
}

/// Set the B and D token hashes
///
/// ### Arguments
/// * `b_token_hash` - The hash of the WASM b_token implementation
/// * `d_token_hash` - The hash of the WASM d_token implementation
pub fn set_token_hashes(e: &Env, b_token_hash: &BytesN<32>, d_token_hash: &BytesN<32>) {
    let key = PoolDataKey::TokenHash;
    e.storage().set::<PoolDataKey, (BytesN<32>, BytesN<32>)>(
        &key,
        &(b_token_hash.clone(), d_token_hash.clone()),
    );
}

/********** Pool Config **********/

/// Fetch the pool configuration
///
/// ### Errors
/// If the pool's config is not set
pub fn get_pool_config(e: &Env) -> PoolConfig {
    e.storage().get_unchecked(&PoolDataKey::PoolConfig).unwrap()
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
/// ### Errors
/// If the reserve does not exist
pub fn get_res_config(e: &Env, asset: &BytesN<32>) -> ReserveConfig {
    let key = PoolDataKey::ResConfig(asset.clone());
    e.storage()
        .get::<PoolDataKey, ReserveConfig>(&key)
        .unwrap()
        .unwrap()
}

/// Set the reserve configuration for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `config` - The reserve configuration for the asset
pub fn set_res_config(e: &Env, asset: &BytesN<32>, config: &ReserveConfig) {
    let key = PoolDataKey::ResConfig(asset.clone());

    e.storage().set::<PoolDataKey, ReserveConfig>(&key, &config);
}

/// Checks if a reserve exists for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
pub fn has_res(e: &Env, asset: &BytesN<32>) -> bool {
    let key = PoolDataKey::ResConfig(asset.clone());
    e.storage().has(&key)
}

/********** Reserve Data (ResData) **********/

/// Fetch the reserve data for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
///
/// ### Errors
/// If the reserve does not exist
pub fn get_res_data(e: &Env, asset: &BytesN<32>) -> ReserveData {
    let key = PoolDataKey::ResData(asset.clone());
    e.storage()
        .get::<PoolDataKey, ReserveData>(&key)
        .unwrap()
        .unwrap()
}

/// Set the reserve data for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `data` - The reserve data for the asset
pub fn set_res_data(e: &Env, asset: &BytesN<32>, data: &ReserveData) {
    let key = PoolDataKey::ResData(asset.clone());
    e.storage().set::<PoolDataKey, ReserveData>(&key, data);
}

/********** Reserve List (ResList) **********/

/// Fetch the list of reserves
pub fn get_res_list(e: &Env) -> Vec<BytesN<32>> {
    e.storage()
        .get::<PoolDataKey, Vec<BytesN<32>>>(&PoolDataKey::ResList)
        .unwrap_or(Ok(vec![e])) // empty vec if nothing exists
        .unwrap()
}

/// Add a reserve to the back of the list and returns the index
///
/// ### Arguments
/// * `asset` - The contract address of the underlying asset
///
/// ### Errors
/// If the number of reserves in the list exceeds 32
///
// @dev: Once added it can't be removed
pub fn push_res_list(e: &Env, asset: &BytesN<32>) -> u32 {
    let mut res_list = get_res_list(e);
    if res_list.len() == 32 {
        panic!("too many reserves")
    }
    res_list.push_back(asset.clone());
    let new_index = res_list.len() - 1;
    e.storage()
        .set::<PoolDataKey, Vec<BytesN<32>>>(&PoolDataKey::ResList, &res_list);
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
        Some(data) => Some(data.unwrap()),
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
        Some(data) => Some(data.unwrap()),
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

/********** UserConfig **********/

/// Fetch the users reserve config
///
/// ### Arguments
/// * `user` - The address of the user
pub fn get_user_config(e: &Env, user: &Address) -> u128 {
    let key = PoolDataKey::UserConfig(user.clone());
    e.storage()
        .get::<PoolDataKey, u128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

/// Set the users reserve config
///
/// ### Arguments
/// * `user` - The address of the user
/// * `config` - The reserve config for the user
pub fn set_user_config(e: &Env, user: &Address, config: &u128) {
    let key = PoolDataKey::UserConfig(user.clone());
    e.storage().set::<PoolDataKey, u128>(&key, config);
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
        Some(data) => Some(data.unwrap()),
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
        .unwrap()
        .unwrap()
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

/// Fetch the pool emission configuration
pub fn get_pool_emission_config(e: &Env) -> PoolEmissionConfig {
    let key = PoolDataKey::PEConfig;
    e.storage()
        .get::<PoolDataKey, PoolEmissionConfig>(&key)
        .unwrap()
        .unwrap()
}

/// Set the pool emission configuration
///
/// ### Arguments
/// * `config` - The pool's emission configuration
pub fn set_pool_emission_config(e: &Env, config: &PoolEmissionConfig) {
    let key = PoolDataKey::PEConfig;
    e.storage()
        .set::<PoolDataKey, PoolEmissionConfig>(&key, config);
}

/********** Auctions ***********/

/// Fetch the auction data for an auction
///
/// ### Arguments
/// * `auction_type` - The type of auction
/// * `user` - The user who is auctioning off assets
///
/// ### Errors
/// If the auction does not exist
pub fn get_auction(e: &Env, auction_type: &u32, user: &Address) -> AuctionData {
    let key = PoolDataKey::Auction(AuctionKey {
        user: user.clone(),
        auct_type: auction_type.clone(),
    });
    e.storage()
        .get::<PoolDataKey, AuctionData>(&key)
        .unwrap()
        .unwrap()
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
