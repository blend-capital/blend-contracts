use soroban_sdk::{
    contracttype, map, unwrap::UnwrapOptimized, vec, Address, Env, Map, Symbol, Vec,
};

use crate::{auctions::AuctionData, pool::Positions};

pub(crate) const INSTANCE_BUMP_AMOUNT: u32 = 34560; // 2 days
pub(crate) const SHARED_BUMP_AMOUNT: u32 = 69120; // 4 days
pub(crate) const CYCLE_BUMP_AMOUNT: u32 = 69120; // 10 days - use for shared data accessed on the 7-day cycle window
pub(crate) const USER_BUMP_AMOUNT: u32 = 518400; // 30 days

/********** Storage Types **********/

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
    // A map of underlying asset's contract address to reserve config
    ResConfig(Address),
    // A map of underlying asset's contract address to reserve data
    ResData(Address),
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

/// Bump the instance rent for the contract
pub fn bump_instance(e: &Env) {
    e.storage().instance().bump(INSTANCE_BUMP_AMOUNT);
}

/********** User **********/

/// Fetch the user's positions or return an empty Positions struct
///
/// ### Arguments
/// * `user` - The address of the user
pub fn get_user_positions(e: &Env, user: &Address) -> Positions {
    let key = PoolDataKey::Positions(user.clone());
    e.storage().persistent().bump(&key, USER_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolDataKey, Positions>(&key)
        .unwrap_or(Positions::env_default(e))
}

/// Set the user's positions
///
/// ### Arguments
/// * `user` - The address of the user
/// * `positions` - The new positions for the user
pub fn set_user_positions(e: &Env, user: &Address, positions: &Positions) {
    let key = PoolDataKey::Positions(user.clone());
    e.storage()
        .persistent()
        .set::<PoolDataKey, Positions>(&key, positions);
}

/********** Admin **********/

// Fetch the current admin Address
///
/// ### Panics
/// If the admin does not exist
pub fn get_admin(e: &Env) -> Address {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&Symbol::new(e, "Admin"), SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&Symbol::new(e, "Admin"))
        .unwrap_optimized()
}

/// Set a new admin
///
/// ### Arguments
/// * `new_admin` - The Address for the admin
pub fn set_admin(e: &Env, new_admin: &Address) {
    e.storage()
        .persistent()
        .set::<Symbol, Address>(&Symbol::new(e, "Admin"), new_admin);
}

/// Checks if an admin is set
pub fn has_admin(e: &Env) -> bool {
    e.storage().persistent().has(&Symbol::new(e, "Admin"))
}

/********** Metadata **********/

/// Set a pool name
///
/// ### Arguments
/// * `name` - The Name of the pool
pub fn set_name(e: &Env, name: &Symbol) {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&Symbol::new(e, "Name"), USER_BUMP_AMOUNT * 10); // 300 days - this can't be updated again
    e.storage()
        .persistent()
        .set::<Symbol, Symbol>(&Symbol::new(e, "Name"), name);
}

/********** Backstop **********/

/// Fetch the backstop ID for the pool
///
/// ### Panics
/// If no backstop is set
pub fn get_backstop(e: &Env) -> Address {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&Symbol::new(e, "Backstop"), SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&Symbol::new(e, "Backstop"))
        .unwrap_optimized()
}

/// Set a new backstop ID
///
/// ### Arguments
/// * `backstop` - The address of the backstop
pub fn set_backstop(e: &Env, backstop: &Address) {
    e.storage()
        .persistent()
        .set::<Symbol, Address>(&Symbol::new(e, "Backstop"), backstop);
}

/********** External Token Contracts **********/

/// Fetch the BLND token ID
pub fn get_blnd_token(e: &Env) -> Address {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&Symbol::new(e, "BLNDTkn"), SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&Symbol::new(e, "BLNDTkn"))
        .unwrap_optimized()
}

/// Set a new BLND token ID
///
/// ### Arguments
/// * `blnd_token_id` - The ID of the BLND token
pub fn set_blnd_token(e: &Env, blnd_token_id: &Address) {
    e.storage()
        .persistent()
        .set::<Symbol, Address>(&Symbol::new(e, "BLNDTkn"), blnd_token_id);
}

/// Fetch the USDC token ID
pub fn get_usdc_token(e: &Env) -> Address {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&Symbol::new(e, "USDCTkn"), SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&Symbol::new(e, "USDCTkn"))
        .unwrap_optimized()
}

/// Set a new USDC token ID
///
/// ### Arguments
/// * `usdc_token_id` - The ID of the USDC token
pub fn set_usdc_token(e: &Env, usdc_token_id: &Address) {
    e.storage()
        .persistent()
        .set::<Symbol, Address>(&Symbol::new(e, "USDCTkn"), usdc_token_id);
}

/********** Pool Config **********/

/// Fetch the pool configuration
///
/// ### Panics
/// If the pool's config is not set
pub fn get_pool_config(e: &Env) -> PoolConfig {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&Symbol::new(e, "PoolConfig"), SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&Symbol::new(e, "PoolConfig"))
        .unwrap_optimized()
}

/// Set the pool configuration
///
/// ### Arguments
/// * `config` - The contract address of the oracle
pub fn set_pool_config(e: &Env, config: &PoolConfig) {
    e.storage()
        .persistent()
        .set::<Symbol, PoolConfig>(&Symbol::new(e, "PoolConfig"), config);
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
    e.storage().persistent().bump(&key, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolDataKey, ReserveConfig>(&key)
        .unwrap_optimized()
}

/// Set the reserve configuration for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `config` - The reserve configuration for the asset
pub fn set_res_config(e: &Env, asset: &Address, config: &ReserveConfig) {
    let key = PoolDataKey::ResConfig(asset.clone());

    e.storage()
        .persistent()
        .set::<PoolDataKey, ReserveConfig>(&key, config);
}

/// Checks if a reserve exists for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
pub fn has_res(e: &Env, asset: &Address) -> bool {
    let key = PoolDataKey::ResConfig(asset.clone());
    e.storage().persistent().has(&key)
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
    e.storage().persistent().bump(&key, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolDataKey, ReserveData>(&key)
        .unwrap_optimized()
}

/// Set the reserve data for an asset
///
/// ### Arguments
/// * `asset` - The contract address of the asset
/// * `data` - The reserve data for the asset
pub fn set_res_data(e: &Env, asset: &Address, data: &ReserveData) {
    let key = PoolDataKey::ResData(asset.clone());
    e.storage()
        .persistent()
        .set::<PoolDataKey, ReserveData>(&key, data);
}

/********** Reserve List (ResList) **********/

/// Fetch the list of reserves
pub fn get_res_list(e: &Env) -> Vec<Address> {
    let key = Symbol::new(e, "ResList");
    e.storage().persistent().bump(&key, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<Symbol, Vec<Address>>(&key)
        .unwrap_or(vec![e]) // empty vec if nothing exists
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
        .persistent()
        .set::<Symbol, Vec<Address>>(&Symbol::new(e, "ResList"), &res_list);
    new_index
}

/********** Reserve Emissions **********/

/// Fetch the emission config for the reserve b or d token
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
pub fn get_res_emis_config(e: &Env, res_token_index: &u32) -> Option<ReserveEmissionsConfig> {
    let key = PoolDataKey::EmisConfig(*res_token_index);
    e.storage().persistent().bump(&key, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolDataKey, ReserveEmissionsConfig>(&key)
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
    let key = PoolDataKey::EmisConfig(*res_token_index);
    e.storage()
        .persistent()
        .set::<PoolDataKey, ReserveEmissionsConfig>(&key, res_emis_config);
}

/// Fetch the emission data for the reserve b or d token
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
pub fn get_res_emis_data(e: &Env, res_token_index: &u32) -> Option<ReserveEmissionsData> {
    let key = PoolDataKey::EmisData(*res_token_index);
    e.storage().persistent().bump(&key, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolDataKey, ReserveEmissionsData>(&key)
}

/// Checks if the reserve token has emissions data
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
pub fn has_res_emis_data(e: &Env, res_token_index: &u32) -> bool {
    let key = PoolDataKey::EmisData(*res_token_index);
    e.storage().persistent().has(&key)
}

/// Set the emission data for the reserve b or d token
///
/// ### Arguments
/// * `res_token_index` - The d/bToken index for the reserve
/// * `res_emis_data` - The new emission data for the reserve token
pub fn set_res_emis_data(e: &Env, res_token_index: &u32, res_emis_data: &ReserveEmissionsData) {
    let key = PoolDataKey::EmisData(*res_token_index);
    e.storage()
        .persistent()
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
        reserve_id: *res_token_index,
    });
    e.storage().persistent().bump(&key, USER_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolDataKey, UserEmissionData>(&key)
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
        reserve_id: *res_token_index,
    });
    e.storage()
        .persistent()
        .set::<PoolDataKey, UserEmissionData>(&key, data)
}

/********** Pool Emissions **********/

/// Fetch the pool reserve emissions
pub fn get_pool_emissions(e: &Env) -> Map<u32, u64> {
    let key = Symbol::new(e, "PoolEmis");
    e.storage().persistent().bump(&key, CYCLE_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<Symbol, Map<u32, u64>>(&key)
        .unwrap_or(map![e])
}

/// Set the pool reserve emissions
///
/// ### Arguments
/// * `emissions` - The map of emissions by reserve token id to EPS
pub fn set_pool_emissions(e: &Env, emissions: &Map<u32, u64>) {
    e.storage()
        .persistent()
        .set::<Symbol, Map<u32, u64>>(&Symbol::new(e, "PoolEmis"), emissions);
}

/// Fetch the pool emission expiration timestamps
pub fn get_pool_emissions_expiration(e: &Env) -> u64 {
    let key = Symbol::new(e, "EmisExp");
    e.storage().persistent().bump(&key, CYCLE_BUMP_AMOUNT);
    e.storage()
        .temporary()
        .get::<Symbol, u64>(&key)
        .unwrap_or(0)
}

/// Set the pool emission configuration
///
/// ### Arguments
/// * `expiration` - The pool's emission configuration
pub fn set_pool_emissions_expiration(e: &Env, expiration: &u64) {
    e.storage()
        .temporary()
        .set::<Symbol, u64>(&Symbol::new(e, "EmisExp"), expiration);
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
        auct_type: *auction_type,
    });
    e.storage()
        .temporary()
        .get::<PoolDataKey, AuctionData>(&key)
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
        auct_type: *auction_type,
    });
    e.storage().temporary().has(&key)
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
        auct_type: *auction_type,
    });
    e.storage()
        .temporary()
        .set::<PoolDataKey, AuctionData>(&key, auction_data);
    e.storage().temporary().bump(&key, INSTANCE_BUMP_AMOUNT);
}

/// Remove an auction
///
/// ### Arguments
/// * `auction_type` - The type of auction
/// * `user` - The user who is auctioning off assets
pub fn del_auction(e: &Env, auction_type: &u32, user: &Address) {
    let key = PoolDataKey::Auction(AuctionKey {
        user: user.clone(),
        auct_type: *auction_type,
    });
    e.storage().temporary().remove(&key);
}
