use crate::{
    auctions::{self, AuctionData},
    emissions::{self, ReserveEmissionMetadata},
    pool::{self, Positions, Request},
    storage::{self, ReserveConfig},
};
use soroban_sdk::{contract, contractclient, contractimpl, Address, Env, String, Symbol, Vec};

/// ### Pool
///
/// An isolated money market pool.
#[contract]
pub struct PoolContract;

#[contractclient(name = "PoolClient")]
pub trait Pool {
    /// Initialize the pool
    ///
    /// ### Arguments
    /// Creator supplied:
    /// * `admin` - The Address for the admin
    /// * `name` - The name of the pool
    /// * `oracle` - The contract address of the oracle
    /// * `backstop_take_rate` - The take rate for the backstop (7 decimals)
    /// * `max_positions` - The maximum number of positions a user is permitted to have
    ///
    /// Pool Factory supplied:
    /// * `backstop_id` - The contract address of the pool's backstop module
    /// * `blnd_id` - The contract ID of the BLND token
    #[allow(clippy::too_many_arguments)]
    fn initialize(
        e: Env,
        admin: Address,
        name: String,
        oracle: Address,
        bstop_rate: u32,
        max_positions: u32,
        backstop_id: Address,
        blnd_id: Address,
    );

    /// (Admin only) Set a new address as the admin of this pool
    ///
    /// ### Arguments
    /// * `new_admin` - The new admin address
    ///
    /// ### Panics
    /// If the caller is not the admin
    fn set_admin(e: Env, new_admin: Address);

    /// (Admin only) Update the pool
    ///
    /// ### Arguments
    /// * `backstop_take_rate` - The new take rate for the backstop (7 decimals)
    /// * `max_positions` - The new maximum number of allowed positions for a single user's account
    ///
    /// ### Panics
    /// If the caller is not the admin
    fn update_pool(e: Env, backstop_take_rate: u32, max_positions: u32);

    /// (Admin only) Queues setting data for a reserve in the pool
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    /// * `config` - The ReserveConfig for the reserve
    ///
    /// ### Panics
    /// If the caller is not the admin
    fn queue_set_reserve(e: Env, asset: Address, metadata: ReserveConfig);

    /// (Admin only) Cancels the queued set of a reserve in the pool
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    ///
    /// ### Panics
    /// If the caller is not the admin or the reserve is not queued for initialization
    fn cancel_set_reserve(e: Env, asset: Address);

    /// (Admin only) Executes the queued set of a reserve in the pool
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    ///
    /// ### Panics
    /// If the reserve is not queued for initialization
    /// or is already setup
    /// or has invalid metadata
    fn set_reserve(e: Env, asset: Address) -> u32;

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

    /// Update the pool status based on the backstop state - backstop triggered status' are odd numbers
    /// * 1 = backstop active - if the minimum backstop deposit has been reached
    ///                and 30% of backstop deposits are not queued for withdrawal
    ///                then all pool operations are permitted
    /// * 3 = backstop on-ice - if the minimum backstop deposit has not been reached
    ///                or 30% of backstop deposits are queued for withdrawal and admin active isn't set
    ///                or 50% of backstop deposits are queued for withdrawal
    ///                then borrowing and cancelling liquidations are not permitted
    /// * 5 = backstop frozen - if 60% of backstop deposits are queued for withdrawal and admin on-ice isn't set
    ///                or 75% of backstop deposits are queued for withdrawal
    ///                then all borrowing, cancelling liquidations, and supplying are not permitted
    ///
    /// ### Panics
    /// If the pool is currently on status 4, "admin-freeze", where only the admin
    /// can perform a status update via `set_status`
    fn update_status(e: Env) -> u32;

    /// (Admin only) Pool status is changed to "pool_status"
    /// * 0 = admin active - requires that the backstop threshold is met
    ///                 and less than 50% of backstop deposits are queued for withdrawal
    /// * 2 = admin on-ice - requires that less than 75% of backstop deposits are queued for withdrawal
    /// * 4 = admin frozen - can always be set
    ///
    /// ### Arguments
    /// * 'pool_status' - The pool status to be set
    ///
    /// ### Panics
    /// If the caller is not the admin
    /// If the specified conditions are not met for the status to be set
    fn set_status(e: Env, pool_status: u32);

    /********* Emission Functions **********/

    /// Consume emissions from the backstop and distribute to the reserves based
    /// on the reserve emission configuration.
    ///
    /// Returns amount of new tokens emitted
    fn gulp_emissions(e: Env) -> i128;

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

    /// Fetch an auction from the ledger. Returns a quote based on the current block.
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction, 0 for liquidation auction, 1 for bad debt auction, and 2 for interest auction
    /// * `user` - The Address involved in the auction
    ///
    /// ### Panics
    /// If the auction does not exist
    fn get_auction(e: Env, auction_type: u32, user: Address) -> AuctionData;

    /// Creates a new bad debt auction
    ///
    ///
    /// ### Panics
    /// If the auction was unable to be created
    fn new_bad_debt_auction(e: Env) -> AuctionData;

    /// Creates a new interest auction
    ///
    /// ### Arguments
    /// * `assets` - The assets interest is being auctioned off for
    ///
    /// ### Panics
    /// If the auction was unable to be created
    fn new_interest_auction(e: Env, assets: Vec<Address>) -> AuctionData;
}

#[contractimpl]
impl Pool for PoolContract {
    #[allow(clippy::too_many_arguments)]
    fn initialize(
        e: Env,
        admin: Address,
        name: String,
        oracle: Address,
        bstop_rate: u32,
        max_postions: u32,
        backstop_id: Address,
        blnd_id: Address,
    ) {
        storage::extend_instance(&e);
        admin.require_auth();

        pool::execute_initialize(
            &e,
            &admin,
            &name,
            &oracle,
            &bstop_rate,
            &max_postions,
            &backstop_id,
            &blnd_id,
        );
    }

    fn set_admin(e: Env, new_admin: Address) {
        storage::extend_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();
        new_admin.require_auth();

        storage::set_admin(&e, &new_admin);

        e.events()
            .publish((Symbol::new(&e, "set_admin"), admin), new_admin);
    }

    fn update_pool(e: Env, backstop_take_rate: u32, max_positions: u32) {
        storage::extend_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();

        pool::execute_update_pool(&e, backstop_take_rate, max_positions);

        e.events().publish(
            (Symbol::new(&e, "update_pool"), admin),
            (backstop_take_rate, max_positions),
        );
    }

    fn queue_set_reserve(e: Env, asset: Address, metadata: ReserveConfig) {
        storage::extend_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();

        pool::execute_queue_set_reserve(&e, &asset, &metadata);

        e.events().publish(
            (Symbol::new(&e, "queue_set_reserve"), admin),
            (asset, metadata),
        );
    }

    fn cancel_set_reserve(e: Env, asset: Address) {
        storage::extend_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();

        pool::execute_cancel_queued_set_reserve(&e, &asset);

        e.events()
            .publish((Symbol::new(&e, "cancel_set_reserve"), admin), asset);
    }

    fn set_reserve(e: Env, asset: Address) -> u32 {
        let index = pool::execute_set_reserve(&e, &asset);

        e.events()
            .publish((Symbol::new(&e, "set_reserve"),), (asset, index));
        index
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
        storage::extend_instance(&e);
        spender.require_auth();
        if from != spender {
            from.require_auth();
        }

        pool::execute_submit(&e, &from, &spender, &to, requests)
    }

    fn bad_debt(e: Env, user: Address) {
        pool::transfer_bad_debt_to_backstop(&e, &user);
    }

    fn update_status(e: Env) -> u32 {
        storage::extend_instance(&e);
        let new_status = pool::execute_update_pool_status(&e);

        e.events()
            .publish((Symbol::new(&e, "set_status"),), new_status);
        new_status
    }

    fn set_status(e: Env, pool_status: u32) {
        storage::extend_instance(&e);
        let admin = storage::get_admin(&e);
        admin.require_auth();
        pool::execute_set_pool_status(&e, pool_status);
        e.events()
            .publish((Symbol::new(&e, "set_status"), admin), pool_status);
    }

    /********* Emission Functions **********/

    fn gulp_emissions(e: Env) -> i128 {
        storage::extend_instance(&e);
        let next_expiration = emissions::gulp_emissions(&e);

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
        storage::extend_instance(&e);
        from.require_auth();

        let amount_claimed = emissions::execute_claim(&e, &from, &reserve_token_ids, &to);

        e.events().publish(
            (Symbol::new(&e, "claim"), from),
            (reserve_token_ids, amount_claimed),
        );

        amount_claimed
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

    fn get_auction(e: Env, auction_type: u32, user: Address) -> AuctionData {
        storage::get_auction(&e, &auction_type, &user)
    }

    fn new_bad_debt_auction(e: Env) -> AuctionData {
        storage::extend_instance(&e);
        let auction_data = auctions::create_bad_debt_auction(&e);

        e.events().publish(
            (Symbol::new(&e, "new_auction"), 1 as u32),
            auction_data.clone(),
        );

        auction_data
    }

    fn new_interest_auction(e: Env, assets: Vec<Address>) -> AuctionData {
        storage::extend_instance(&e);
        let auction_data = auctions::create_interest_auction(&e, &assets);

        e.events().publish(
            (Symbol::new(&e, "new_auction"), 2 as u32),
            auction_data.clone(),
        );

        auction_data
    }
}
