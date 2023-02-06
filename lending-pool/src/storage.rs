use soroban_auth::Identifier;
use soroban_sdk::{contracttype, vec, BytesN, Env, Map, Vec};

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

/// The configuration information about a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveConfig {
    pub b_token: BytesN<32>, // the address of the bToken contract
    pub d_token: BytesN<32>, // the address of the dToken contract
    pub decimals: u32,       // the decimals used in both the bToken and underlying contract
    pub c_factor: u32,       // the collateral factor for the reserve
    pub l_factor: u32,       // the liability factor for the reserve
    pub util: u32,           // the target utilization rate
    pub r_one: u32,          // the R1 value in the interest rate formula
    pub r_two: u32,          // the R2 value in the interest rate formula
    pub r_three: u32,        // the R3 value in the interest rate formula
    pub reactivity: u32,     // the reactivity constant for the reserve
    pub index: u32,          // the index of the reserve in the list (TODO: Make u8)
}

/// The data for a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveData {
    // TODO: These rates are correlated and can be simplified if both the b/dTokens have a totalSupply
    pub b_rate: i128, // the conversion rate from bToken to underlying - NOTE: stored as 9 decimals
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
    pub index: u64,
    pub last_time: u64,
}

/// The user emission data for the reserve b or d token
#[derive(Clone)]
#[contracttype]
pub struct UserEmissionData {
    pub index: u64,
    pub accrued: u64,
}

/// The data for auctions
#[derive(Clone)]
#[contracttype]
pub struct AuctionData {
    pub strt_block: u32,          // the block the auction starts on
    pub auct_type: u32,           // the type of auction
    pub ask_count: u32, // the number of assets being sold by contract and purchased by user
    pub ask_ids: Vec<BytesN<32>>, // a vector of ids for the assets being sold by contract and purchased by user
    pub bid_count: u32, // the number of assets being purchased by contract and sold by user
    pub bid_ids: Vec<BytesN<32>>, // a vector of ids for the assets being purchased by contract and sold by user
    pub bid_ratio: u64,           // the ratio of user bad_debt to bid_amount
}

/********** Storage Key Types **********/

#[derive(Clone)]
#[contracttype]
pub struct UserReserveKey {
    user: Identifier,
    reserve_id: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct AuctionKey {
    user: Identifier, // the Identifier whose assets are involved in the auction
    auct_type: u32,   // the type of auction taking place
}

// TODO: See if we can avoid publishing this
#[derive(Clone)]
#[contracttype]
pub enum PoolDataKey {
    // The address that can manage the pool
    Admin,
    // The backstop address for the pool
    Backstop,
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
    UserConfig(Identifier),
    // The emission information for a reserve asset for a user
    UserEmis(UserReserveKey),
    // The auction's data
    Auction(AuctionKey),
    // A list of auctions and their associated data
    AuctData(Identifier),
}

/********** Storage **********/

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

    /********** Backstop **********/

    /// Fetch the backstop for the pool
    ///
    /// ### Errors
    /// If no backstop is set
    fn get_backstop(&self) -> BytesN<32>;

    /// Set a new admin
    ///
    /// ### Arguments
    /// * `backstop` - The address of the backstop
    fn set_backstop(&self, new_admin: BytesN<32>);

    /********** Pool Config **********/

    /// Fetch the pool configuration
    ///
    /// ### Errors
    /// If the pool's config is not set
    fn get_pool_config(&self) -> PoolConfig;

    /// Set the pool configuration
    ///
    /// ### Arguments
    /// * `config` - The contract address of the oracle
    fn set_pool_config(&self, config: PoolConfig);

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

    /********** Reserve Emissions **********/

    /// Fetch the emission config for the reserve b or d token
    ///
    /// ### Arguments
    /// * `res_token_index` - The d/bToken index for the reserve
    fn get_res_emis_config(&self, res_token_index: u32) -> Option<ReserveEmissionsConfig>;

    /// Set the emission config for the reserve b or d token
    ///
    /// ### Arguments
    /// * `res_token_index` - The d/bToken index for the reserve
    /// * `res_emis_config` - The new emission config for the reserve token
    fn set_res_emis_config(&self, res_token_index: u32, res_emis_config: ReserveEmissionsConfig);

    /// Fetch the emission data for the reserve b or d token
    ///
    /// ### Arguments
    /// * `res_token_index` - The d/bToken index for the reserve
    fn get_res_emis_data(&self, res_token_index: u32) -> Option<ReserveEmissionsData>;

    /// Checks if the reserve token has emissions data
    ///
    /// ### Arguments
    /// * `res_token_index` - The d/bToken index for the reserve
    fn has_res_emis_data(&self, res_token_index: u32) -> bool;

    /// Set the emission data for the reserve b or d token
    ///
    /// ### Arguments
    /// * `res_token_index` - The d/bToken index for the reserve
    /// * `res_emis_data` - The new emission data for the reserve token
    fn set_res_emis_data(&self, res_token_index: u32, res_emis_data: ReserveEmissionsData);

    /********** UserConfig **********/

    /// Fetch the users reserve config
    ///
    /// ### Arguments
    /// * `user` - The address of the user
    fn get_user_config(&self, user: Identifier) -> u128;

    /// Set the users reserve config
    ///
    /// ### Arguments
    /// * `user` - The address of the user
    /// * `config` - The reserve config for the user
    fn set_user_config(&self, user: Identifier, config: u128);

    /********** User Emissions **********/

    /// Fetch the users emission data for a reserve's b or d token
    ///
    /// ### Arguments
    /// * `user` - The address of the user
    /// * `res_token_index` - The d/bToken index for the reserve
    fn get_user_emissions(
        &self,
        user: Identifier,
        res_token_index: u32,
    ) -> Option<UserEmissionData>;

    /// Set the users emission data for a reserve's d or d token
    ///
    /// ### Arguments
    /// * `user` - The address of the user
    /// * `res_token_index` - The d/bToken index for the reserve
    /// * `data` - The new user emission d ata for the d/bToken
    fn set_user_emissions(&self, user: Identifier, res_token_index: u32, data: UserEmissionData);

    /********** Pool Emissions **********/

    /// Fetch the pool reserve emissions
    fn get_pool_emissions(&self) -> Map<u32, u64>;

    /// Set the pool reserve emissions
    ///
    /// ### Arguments
    /// * `emissions` - The map of emissions by reserve token id to EPS
    fn set_pool_emissions(&self, emissions: Map<u32, u64>);

    /// Fetch the pool emission configuration
    fn get_pool_emission_config(&self) -> PoolEmissionConfig;

    /// Set the pool emission configuration
    ///
    /// ### Arguments
    /// * `config` - The pool's emission configuration
    fn set_pool_emission_config(&self, config: PoolEmissionConfig);

    /******** Auction *********/

    /// Fetch the starting block for an auction
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    /// * `user` - The user who is auctioning off assets
    ///
    /// ### Errors
    /// If the auction does not exist
    fn get_auction(&self, auction_type: u32, user: Identifier) -> u32;

    /// Check if an auction exists for the given type and user
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    /// * `user` - The user who is auctioning off assets
    fn has_auction(&self, auction_type: u32, user: Identifier) -> bool;

    /// Set the the starting block for an auction
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    /// * `user` - The user who is auctioning off assets
    /// * `start_block` - The starting block for the auction
    fn set_auction(&self, auction_type: u32, user: Identifier, start_block: u32);

    /// Remove an auction
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    /// * `user` - The user who is auctioning off assets
    fn del_auction(&self, auction_type: u32, user: Identifier);

    /******** Auction Data *********/

    // TODO: Remove once auction code has been ported

    /// Fetch the data for an auction
    ///
    /// ### Arguments
    /// * `auction_id` - The auction id
    ///
    /// Errors
    /// If the auction does not exist
    fn get_auction_data(&self, auction_id: Identifier) -> AuctionData;

    /// Set the data for an auction
    ///
    /// ### Arguments
    /// * `auction_id` - The auction id
    /// * `data` - The auction data
    fn set_auction_data(&self, auction_id: Identifier, data: AuctionData);

    /// remove the data for an auction
    ///
    /// ### Arguments
    /// * `auction_id` - The auction id
    fn remove_auction_data(&self, auction_id: Identifier);
}

pub struct StorageManager(Env);

impl PoolDataStore for StorageManager {
    /********** Admin **********/

    fn get_admin(&self) -> Identifier {
        self.env()
            .storage()
            .get_unchecked(PoolDataKey::Admin)
            .unwrap()
    }

    fn set_admin(&self, new_admin: Identifier) {
        self.env()
            .storage()
            .set::<PoolDataKey, Identifier>(PoolDataKey::Admin, new_admin);
    }

    fn has_admin(&self) -> bool {
        self.env().storage().has(PoolDataKey::Admin)
    }

    /********** Backstop **********/

    fn get_backstop(&self) -> BytesN<32> {
        self.env()
            .storage()
            .get_unchecked(PoolDataKey::Backstop)
            .unwrap()
    }

    fn set_backstop(&self, backstop: BytesN<32>) {
        self.env()
            .storage()
            .set::<PoolDataKey, BytesN<32>>(PoolDataKey::Backstop, backstop);
    }

    /********** Pool Config **********/

    fn get_pool_config(&self) -> PoolConfig {
        self.env()
            .storage()
            .get_unchecked(PoolDataKey::PoolConfig)
            .unwrap()
    }

    fn set_pool_config(&self, config: PoolConfig) {
        let key = PoolDataKey::PoolConfig;
        self.env()
            .storage()
            .set::<PoolDataKey, PoolConfig>(key, config);
    }

    /********** Reserve Config (ResConfig) **********/

    fn get_res_config(&self, asset: BytesN<32>) -> ReserveConfig {
        let key = PoolDataKey::ResConfig(asset);
        self.env()
            .storage()
            .get::<PoolDataKey, ReserveConfig>(key)
            .unwrap()
            .unwrap()
    }

    fn set_res_config(&self, asset: BytesN<32>, config: ReserveConfig) {
        let key = PoolDataKey::ResConfig(asset.clone());
        let mut indexed_config = config.clone();

        // TODO: Might fit better in reserve module
        // add to reserve list if its new
        if !self.env().storage().has(key.clone()) {
            let index = self.push_res_list(asset);
            indexed_config.index = index;
        }

        self.env()
            .storage()
            .set::<PoolDataKey, ReserveConfig>(key, indexed_config);
    }

    fn has_res(&self, asset: BytesN<32>) -> bool {
        let key = PoolDataKey::ResConfig(asset);
        self.env().storage().has(key)
    }

    /********** Reserve Data (ResData) **********/

    fn get_res_data(&self, asset: BytesN<32>) -> ReserveData {
        let key = PoolDataKey::ResData(asset);
        self.env()
            .storage()
            .get::<PoolDataKey, ReserveData>(key)
            .unwrap()
            .unwrap()
    }

    fn set_res_data(&self, asset: BytesN<32>, data: ReserveData) {
        let key = PoolDataKey::ResData(asset);
        self.env()
            .storage()
            .set::<PoolDataKey, ReserveData>(key, data);
    }

    /********** Reserve List (ResList) **********/

    fn get_res_list(&self) -> Vec<BytesN<32>> {
        self.env()
            .storage()
            .get::<PoolDataKey, Vec<BytesN<32>>>(PoolDataKey::ResList)
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
        self.env()
            .storage()
            .set::<PoolDataKey, Vec<BytesN<32>>>(PoolDataKey::ResList, res_list);
        new_index
    }

    /********** Reserve Emissions **********/

    fn get_res_emis_config(&self, res_token_index: u32) -> Option<ReserveEmissionsConfig> {
        let key = PoolDataKey::EmisConfig(res_token_index);
        let result = self
            .env()
            .storage()
            .get::<PoolDataKey, ReserveEmissionsConfig>(key);
        match result {
            Some(data) => Some(data.unwrap()),
            None => None,
        }
    }

    fn set_res_emis_config(&self, res_token_index: u32, res_emis_config: ReserveEmissionsConfig) {
        let key = PoolDataKey::EmisConfig(res_token_index);
        self.env()
            .storage()
            .set::<PoolDataKey, ReserveEmissionsConfig>(key, res_emis_config);
    }

    fn get_res_emis_data(&self, res_token_index: u32) -> Option<ReserveEmissionsData> {
        let key = PoolDataKey::EmisData(res_token_index);
        let result = self
            .env()
            .storage()
            .get::<PoolDataKey, ReserveEmissionsData>(key);
        match result {
            Some(data) => Some(data.unwrap()),
            None => None,
        }
    }

    fn has_res_emis_data(&self, res_token_index: u32) -> bool {
        let key = PoolDataKey::EmisData(res_token_index);
        self.env().storage().has(key)
    }

    fn set_res_emis_data(&self, res_token_index: u32, res_emis_data: ReserveEmissionsData) {
        let key = PoolDataKey::EmisData(res_token_index);
        self.env()
            .storage()
            .set::<PoolDataKey, ReserveEmissionsData>(key, res_emis_data);
    }

    /********** UserConfig **********/

    fn get_user_config(&self, user: Identifier) -> u128 {
        let key = PoolDataKey::UserConfig(user);
        self.env()
            .storage()
            .get::<PoolDataKey, u128>(key)
            .unwrap_or(Ok(0))
            .unwrap()
    }

    fn set_user_config(&self, user: Identifier, config: u128) {
        let key = PoolDataKey::UserConfig(user);
        self.env().storage().set::<PoolDataKey, u128>(key, config);
    }

    /********** User Emissions **********/

    fn get_user_emissions(
        &self,
        user: Identifier,
        res_token_index: u32,
    ) -> Option<UserEmissionData> {
        let key = PoolDataKey::UserEmis(UserReserveKey {
            user,
            reserve_id: res_token_index,
        });
        let result = self
            .env()
            .storage()
            .get::<PoolDataKey, UserEmissionData>(key);
        match result {
            Some(data) => Some(data.unwrap()),
            None => None,
        }
    }

    fn set_user_emissions(&self, user: Identifier, res_token_index: u32, data: UserEmissionData) {
        let key = PoolDataKey::UserEmis(UserReserveKey {
            user,
            reserve_id: res_token_index,
        });
        self.env()
            .storage()
            .set::<PoolDataKey, UserEmissionData>(key, data);
    }

    /********** Pool Emissions **********/

    fn get_pool_emissions(&self) -> Map<u32, u64> {
        let key = PoolDataKey::PoolEmis;
        self.env()
            .storage()
            .get::<PoolDataKey, Map<u32, u64>>(key)
            .unwrap()
            .unwrap()
    }

    fn set_pool_emissions(&self, emissions: Map<u32, u64>) {
        let key = PoolDataKey::PoolEmis;
        self.env()
            .storage()
            .set::<PoolDataKey, Map<u32, u64>>(key, emissions);
    }

    fn get_pool_emission_config(&self) -> PoolEmissionConfig {
        let key = PoolDataKey::PEConfig;
        self.env()
            .storage()
            .get::<PoolDataKey, PoolEmissionConfig>(key)
            .unwrap()
            .unwrap()
    }

    fn set_pool_emission_config(&self, config: PoolEmissionConfig) {
        let key = PoolDataKey::PEConfig;
        self.env()
            .storage()
            .set::<PoolDataKey, PoolEmissionConfig>(key, config);
    }

    /********** Auctions ***********/

    fn get_auction(&self, auction_type: u32, user: Identifier) -> u32 {
        let key = PoolDataKey::Auction(AuctionKey {
            user,
            auct_type: auction_type,
        });
        self.env()
            .storage()
            .get::<PoolDataKey, u32>(key)
            .unwrap()
            .unwrap()
    }

    fn has_auction(&self, auction_type: u32, user: Identifier) -> bool {
        let key = PoolDataKey::Auction(AuctionKey {
            user,
            auct_type: auction_type,
        });
        self.env().storage().has(key)
    }

    fn set_auction(&self, auction_type: u32, user: Identifier, start_block: u32) {
        let key = PoolDataKey::Auction(AuctionKey {
            user,
            auct_type: auction_type,
        });
        self.env()
            .storage()
            .set::<PoolDataKey, u32>(key, start_block)
    }

    fn del_auction(&self, auction_type: u32, user: Identifier) {
        let key = PoolDataKey::Auction(AuctionKey {
            user,
            auct_type: auction_type,
        });
        self.env().storage().remove(key)
    }

    // TODO: Old

    fn get_auction_data(&self, auction_id: Identifier) -> AuctionData {
        let key = PoolDataKey::AuctData(auction_id);
        self.env()
            .storage()
            .get::<PoolDataKey, AuctionData>(key)
            .unwrap()
            .unwrap()
    }

    fn set_auction_data(&self, auction_id: Identifier, data: AuctionData) {
        let key = PoolDataKey::AuctData(auction_id);
        self.env()
            .storage()
            .set::<PoolDataKey, AuctionData>(key, data);
    }

    fn remove_auction_data(&self, auction_id: Identifier) {
        let key = PoolDataKey::AuctData(auction_id);
        self.env().storage().remove::<PoolDataKey>(key);
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
