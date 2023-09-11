use crate::{
    auctions::{self, AuctionData},
    emissions::{self, ReserveEmissionMetadata},
    pool::{self, Positions, Request},
    storage::{
        self, PoolConfig, ReserveConfig, ReserveData, ReserveEmissionsConfig, ReserveEmissionsData,
    },
};
use soroban_sdk::{contract, contractimpl, Address, Env, Map, Symbol, Vec};

/// ### Pool
///
/// An isolated money market pool.
#[contract]
pub struct Pool;

pub trait PoolTrait {
    /// Initialize the pool
    ///
    /// ### Arguments
    /// Creator supplied:
    /// * `admin` - The Address for the admin
    /// * `name` - The name of the pool
    /// * `oracle` - The contract address of the oracle
    /// * `backstop_take_rate` - The take rate for the backstop in stroops
    ///
    /// Pool Factory supplied:
    /// * `backstop_id` - The contract address of the pool's backstop module
    /// * `blnd_id` - The contract ID of the BLND token
    /// * `usdc_id` - The contract ID of the BLND token
    #[allow(clippy::too_many_arguments)]
    fn initialize(
        e: Env,
        admin: Address,
        name: Symbol,
        oracle: Address,
        bstop_rate: u64,
        backstop_id: Address,
        blnd_id: Address,
        usdc_id: Address,
    );

    /// (Admin only) Update the pool
    ///
    /// ### Arguments
    /// * `backstop_take_rate` - The new take rate for the backstop
    ///
    /// ### Panics
    /// If the caller is not the admin
    fn update_pool(e: Env, backstiop_take_rate: u64);

    /// (Admin only) Initialize a reserve in the pool
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    /// * `config` - The ReserveConfig for the reserve
    ///
    /// ### Panics
    /// If the caller is not the admin or the reserve is already setup
    fn init_reserve(e: Env, asset: Address, metadata: ReserveConfig);

    /// (Admin only) Update a reserve in the pool
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    /// * `config` - The ReserveConfig for the reserve
    ///
    /// ### Panics
    /// If the caller is not the admin or the reserve does not exist
    fn update_reserve(e: Env, asset: Address, config: ReserveConfig);

    /// Fetch the reserve configuration for a reserve
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    fn get_reserve_config(e: Env, asset: Address) -> ReserveConfig;

    /// Fetch the reserve data for a reserve
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    fn get_reserve_data(e: Env, asset: Address) -> ReserveData;

    /// Fetch the positions for an address
    ///
    /// ### Arguments
    /// * `address` - The address to fetch positions for
    fn get_positions(e: Env, address: Address) -> Positions;

    /// Submit a set of requests to the pool where 'from' takes on the position, 'sender' sends any
    /// required tokens to the pool and 'to' receives any tokens sent from the pool
    ///
    /// Returns the new positions for 'from'
    ///
    /// ### Arguments
    /// * `from` - The address of the user whose positions are being modified
    /// * `spender` - The address of the user who is sending tokens to the pool
    /// * `to` - The address of the user who is receiving tokens from the pool
    /// * `requests` - A vec of requests to be processed
    ///
    /// ### Panics
    /// If the request is not able to be completed for cases like insufficient funds or invalid health factor
    fn submit(
        e: Env,
        from: Address,
        spender: Address,
        to: Address,
        requests: Vec<Request>,
    ) -> Positions;

    /// Manage bad debt. Debt is considered "bad" if there is no longer has any collateral posted.
    ///
    /// To manage a user's bad debt, all collateralized reserves for the user must be liquidated
    /// before debt can be transferred to the backstop.
    ///
    /// To manage a backstop's bad debt, the backstop module must be below a critical threshold
    /// to allow bad debt to be burnt.
    ///
    /// ### Arguments
    /// * `user` - The user who currently possesses bad debt
    ///
    /// ### Panics
    /// If the user has collateral posted
    fn bad_debt(e: Env, user: Address);

    /// Update the pool status based on the backstop state
    /// * 0 = active - if the minimum backstop deposit has been reached
    /// * 1 = on ice - if the minimum backstop deposit has not been reached
    ///                or 25% of backstop deposits are queued for withdrawal
    /// * 2 = frozen - if 50% of backstop deposits are queued for withdrawal
    ///
    /// ### Panics
    /// If the pool is currently of status 3, "admin-freeze", where only the admin
    /// can perform a status update via `set_status`
    fn update_status(e: Env) -> u32;

    /// (Admin only) Pool status is changed to "pool_status"
    /// * 0 = active
    /// * 1 = on ice
    /// * 2 = frozen
    /// * 3 = admin frozen (only the admin can unfreeze)
    ///
    /// ### Arguments
    /// * 'pool_status' - The pool status to be set
    ///
    /// ### Panics
    /// If the caller is not the admin
    fn set_status(e: Env, pool_status: u32);

    /// Fetch the configuration of the pool
    fn get_pool_config(e: Env) -> PoolConfig;

    /********* Emission Functions **********/

    /// Fetch the next emission configuration
    fn get_emissions_config(e: Env) -> Map<u32, u64>;

    /// Update emissions for reserves for the next emission cycle
    ///
    /// Needs to be performed each emission cycle, as determined by the expiration
    ///
    /// Returns the expiration timestamp
    fn update_emissions(e: Env) -> u64;

    /// (Admin only) Set the emission configuration for the pool
    ///
    /// Changes will be applied in the next pool `update_emissions`, and affect the next emission cycle
    ///
    /// ### Arguments
    /// * `res_emission_metadata` - A vector of ReserveEmissionMetadata to update metadata to
    ///
    /// ### Panics
    /// * If the caller is not the admin
    /// * If the sum of ReserveEmissionMetadata shares is greater than 1
    fn set_emissions_config(e: Env, res_emission_metadata: Vec<ReserveEmissionMetadata>);

    /// Claims outstanding emissions for the caller for the given reserve's
    ///
    /// Returns the number of tokens claimed
    ///
    /// ### Arguments
    /// * `from` - The address claiming
    /// * `reserve_token_ids` - Vector of reserve token ids
    /// * `to` - The Address to send the claimed tokens to
    fn claim(e: Env, from: Address, reserve_token_ids: Vec<u32>, to: Address) -> i128;

    /***** Reserve Emission Functions *****/

    /// Fetch the emission details for a given reserve token
    ///
    /// ### Arguments
    /// * `asset` - The contract address of the asset backing the reserve
    /// * `token_type` - The type of reserve token (0 for dToken / 1 for bToken)
    fn get_reserve_emissions(
        e: Env,
        asset: Address,
        token_type: u32,
    ) -> Option<(ReserveEmissionsConfig, ReserveEmissionsData)>;

    /***** Auction / Liquidation Functions *****/

    /// Creates a new user liquidation auction
    ///
    /// ### Arguments
    /// * `user` - The user getting liquidated through the auction
    /// * `percent_liquidated` - The percent of the user's position being liquidated as a percentage (15 => 15%)
    ///
    /// ### Panics
    /// If the user liquidation auction was unable to be created
    fn new_liquidation_auction(e: Env, user: Address, percent_liquidated: u64) -> AuctionData;

    /// Delete a user liquidation auction if the user is no longer eligible to be liquidated.
    ///
    /// ### Arguments
    /// * `user` - The user getting liquidated through the auction
    ///
    /// ### Panics
    /// If the user is still eligible to be liquidated state or the auction doesn't exist
    fn del_liquidation_auction(e: Env, user: Address);

    /// Fetch an auction from the ledger. Returns a quote based on the current block.
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    /// * `user` - The Address involved in the auction
    ///
    /// ### Panics
    /// If the auction does not exist
    fn get_auction(e: Env, auction_type: u32, user: Address) -> AuctionData;

    /// Creates a new auction
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    ///
    /// ### Panics
    /// If the auction was unable to be created
    fn new_auction(e: Env, auction_type: u32) -> AuctionData;
}

#[contractimpl]
impl PoolTrait for Pool {
    #[allow(clippy::too_many_arguments)]
    fn initialize(
        e: Env,
        admin: Address,
        name: Symbol,
        oracle: Address,
        bstop_rate: u64,
        backstop_id: Address,
        blnd_id: Address,
        usdc_id: Address,
    ) {
        admin.require_auth();

        pool::execute_initialize(
            &e,
            &admin,
            &name,
            &oracle,
            &bstop_rate,
            &backstop_id,
            &blnd_id,
            &usdc_id,
        );
    }

    fn update_pool(e: Env, backstop_take_rate: u64) {
        storage::bump_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();

        pool::execute_update_pool(&e, backstop_take_rate);

        e.events()
            .publish((Symbol::new(&e, "update_pool"), admin), backstop_take_rate);
    }

    fn init_reserve(e: Env, asset: Address, config: ReserveConfig) {
        storage::bump_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();

        pool::initialize_reserve(&e, &asset, &config);

        e.events()
            .publish((Symbol::new(&e, "init_reserve"), admin), asset);
    }

    fn update_reserve(e: Env, asset: Address, config: ReserveConfig) {
        storage::bump_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();

        pool::execute_update_reserve(&e, &asset, &config);

        e.events()
            .publish((Symbol::new(&e, "update_reserve"), admin), asset);
    }

    fn get_reserve_config(e: Env, asset: Address) -> ReserveConfig {
        storage::get_res_config(&e, &asset)
    }

    fn get_reserve_data(e: Env, asset: Address) -> ReserveData {
        storage::get_res_data(&e, &asset)
    }

    fn get_positions(e: Env, address: Address) -> Positions {
        storage::get_user_positions(&e, &address)
    }

    fn submit(
        e: Env,
        from: Address,
        spender: Address,
        to: Address,
        requests: Vec<Request>,
    ) -> Positions {
        storage::bump_instance(&e);
        from.require_auth();
        if from != spender {
            spender.require_auth();
        }

        pool::execute_submit(&e, &from, &spender, &to, requests)
    }

    fn bad_debt(e: Env, user: Address) {
        pool::transfer_bad_debt_to_backstop(&e, &user);
    }

    fn update_status(e: Env) -> u32 {
        storage::bump_instance(&e);
        let new_status = pool::execute_update_pool_status(&e);

        e.events()
            .publish((Symbol::new(&e, "set_status"),), new_status);
        new_status
    }

    fn set_status(e: Env, pool_status: u32) {
        storage::bump_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();

        pool::set_pool_status(&e, pool_status);

        e.events()
            .publish((Symbol::new(&e, "set_status"), admin), pool_status);
    }

    fn get_pool_config(e: Env) -> PoolConfig {
        storage::get_pool_config(&e)
    }

    /********* Emission Functions **********/

    // @dev: view
    fn get_emissions_config(e: Env) -> Map<u32, u64> {
        storage::get_pool_emissions(&e)
    }

    fn update_emissions(e: Env) -> u64 {
        storage::bump_instance(&e);
        let next_expiration = pool::update_pool_emissions(&e);

        e.events()
            .publish((Symbol::new(&e, "update_emissions"),), next_expiration);
        next_expiration
    }

    fn set_emissions_config(e: Env, res_emission_metadata: Vec<ReserveEmissionMetadata>) {
        let admin = storage::get_admin(&e);
        admin.require_auth();

        emissions::set_pool_emissions(&e, res_emission_metadata);
    }

    fn claim(e: Env, from: Address, reserve_token_ids: Vec<u32>, to: Address) -> i128 {
        storage::bump_instance(&e);
        from.require_auth();

        let amount_claimed = emissions::execute_claim(&e, &from, &reserve_token_ids, &to);

        e.events().publish(
            (Symbol::new(&e, "claim"), from),
            (reserve_token_ids, amount_claimed),
        );

        amount_claimed
    }

    // @dev: view
    fn get_reserve_emissions(
        e: Env,
        asset: Address,
        token_type: u32,
    ) -> Option<(ReserveEmissionsConfig, ReserveEmissionsData)> {
        emissions::get_reserve_emissions(&e, &asset, token_type)
    }

    /***** Auction / Liquidation Functions *****/

    fn new_liquidation_auction(e: Env, user: Address, percent_liquidated: u64) -> AuctionData {
        let auction_data = auctions::create_liquidation(&e, &user, percent_liquidated);

        e.events().publish(
            (Symbol::new(&e, "new_liquidation_auction"), user),
            auction_data.clone(),
        );
        auction_data
    }

    fn del_liquidation_auction(e: Env, user: Address) {
        auctions::delete_liquidation(&e, &user);

        e.events()
            .publish((Symbol::new(&e, "delete_liquidation_auction"), user), ());
    }

    fn get_auction(e: Env, auction_type: u32, user: Address) -> AuctionData {
        storage::get_auction(&e, &auction_type, &user)
    }

    fn new_auction(e: Env, auction_type: u32) -> AuctionData {
        storage::bump_instance(&e);
        let auction_data = auctions::create(&e, auction_type);

        e.events().publish(
            (Symbol::new(&e, "new_auction"), auction_type),
            auction_data.clone(),
        );

        auction_data
    }
}
