use crate::{
    auctions::{self, AuctionData, AuctionQuote, LiquidationMetadata},
    bad_debt,
    emissions::{self, ReserveEmissionMetadata},
    errors::PoolError,
    pool,
    reserve::Reserve,
    storage::{self, ReserveConfig, ReserveEmissionsConfig, ReserveEmissionsData, ReserveMetadata},
};
use soroban_sdk::{contractimpl, Address, BytesN, Env, Map, Symbol, Vec};

/// ### Pool
///
/// An isolated money market pool.
pub struct PoolContract;

pub trait PoolContractTrait {
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
    /// * `b_token_hash` - The hash of the WASM b_token implementation
    /// * `d_token_hash` - The hash of the WASM d_token implementation
    /// * `blnd_id` - The contract ID of the BLND token
    /// * `usdc_id` - The contract ID of the BLND token
    fn initialize(
        e: Env,
        admin: Address,
        name: Symbol,
        oracle: Address,
        bstop_rate: u64,
        backstop_id: Address,
        b_token_hash: BytesN<32>,
        d_token_hash: BytesN<32>,
        blnd_id: Address,
        usdc_id: Address,
    ) -> Result<(), PoolError>;

    /// Initialize a reserve in the pool
    ///
    /// ### Arguments
    /// * `admin` - The Address for the admin
    /// * `asset` - The underlying asset to add as a reserve
    /// * `metadata` - The ReserveMetadata for the reserve
    ///
    /// ### Errors
    /// If the caller is not the admin or the reserve is already setup
    fn init_reserve(
        e: Env,
        admin: Address,
        asset: Address,
        metadata: ReserveMetadata,
    ) -> Result<(), PoolError>;

    /// Update a reserve in the pool
    ///
    /// ### Arguments
    /// * `admin` - The Address for the admin
    /// * `asset` - The underlying asset to add as a reserve
    /// * `metadata` - The ReserveMetadata for the reserve
    ///
    /// ### Errors
    /// If the caller is not the admin or the reserve does not exist
    fn update_reserve(
        e: Env,
        admin: Address,
        asset: Address,
        metadata: ReserveMetadata,
    ) -> Result<(), PoolError>;

    /// Fetch the reserve configuration for a reserve
    ///
    /// ### Arguments
    /// * `asset` - The underlying asset to add as a reserve
    fn reserve_config(e: Env, asset: Address) -> ReserveConfig;

    /// Fetch the reserve usage configuration for a user
    ///
    /// ### Arguments
    /// * `user` - The Address to fetch the reserve usage for
    fn config(e: Env, user: Address) -> u128;

    /// `from` supplies the `amount` of `asset` into the pool in return for the asset's bToken
    ///
    /// Returns the amount of bTokens minted
    ///
    /// ### Arguments
    /// * `from` - The address supplying
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to supply
    ///
    /// ### Errors
    /// If the invoker has not approved the pool to transfer `asset` at least `amount` and has
    /// enough tokens to do so
    fn supply(e: Env, from: Address, asset: Address, amount: i128) -> Result<i128, PoolError>;

    /// Withdraws from `from` `amount` of the `asset` from the invoker and returns it to the `to` Address
    ///
    /// Returns the amount of bTokens burnt
    ///
    /// ### Arguments
    /// * `from` - The address withdrawing
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to withdraw
    /// * `to` - The address to send the withdrawn funds to
    ///
    /// ### Errors
    /// If the invoker does not have enough funds to burn
    fn withdraw(
        e: Env,
        from: Address,
        asset: Address,
        amount: i128,
        to: Address,
    ) -> Result<i128, PoolError>;

    /// Borrow's `amount` of `asset` from the pool and sends it to the `to` address and credits a debt
    /// to the `from` Address
    ///
    /// Returns the amount of dTokens minted
    ///
    /// ### Arguments
    /// * `from` - The address supplying
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to borrow
    /// * `to` - The address receiving the funds
    fn borrow(
        e: Env,
        from: Address,
        asset: Address,
        amount: i128,
        to: Address,
    ) -> Result<i128, PoolError>;

    /// `from` repays the `amount` of debt for the `asset`, such that the debt is reduced for
    /// the address `on_behalf_of`
    ///
    /// Returns the amount of lTokens burned
    ///
    /// ### Arguments
    /// * `from` - The address repaying
    /// * `asset` - The contract address of the asset
    /// * `amount` - The amount of underlying `asset` tokens to borrow
    ///     * Sending i128.MAX will repay the full amount of the debt
    /// * `on_behalf_of` - The address receiving the funds
    fn repay(
        e: Env,
        from: Address,
        asset: Address,
        amount: i128,
        on_behalf_of: Address,
    ) -> Result<i128, PoolError>;

    /// Fetches the d rate for a given asset
    ///
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    fn get_d_rate(e: Env, asset: Address) -> i128;

    /// Fetches the b rate for a given asset
    ///
    /// ### Arguments
    /// * `asset` - The contract address of the asset
    fn get_b_rate(e: Env, asset: Address) -> i128;

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
    /// ### Errors
    /// If the user has collateral posted
    fn bad_debt(e: Env, user: Address) -> Result<(), PoolError>;

    /// Update the pool status based on the backstop state
    /// * 0 = active - if the minimum backstop deposit has been reached
    /// * 1 = on ice - if the minimum backstop deposit has not been reached
    ///                or 25% of backstop deposits are queued for withdrawal
    /// * 2 = frozen - if 50% of backstop deposits are queued for withdrawal
    ///
    /// ### Errors
    /// If the pool is currently of status 3, "admin-freeze", where only the admin
    /// can perform a status update via `set_status`
    fn update_state(e: Env) -> Result<u32, PoolError>;

    /// Pool status is changed to "pool_status"
    /// * 0 = active
    /// * 1 = on ice
    /// * 2 = frozen
    ///
    /// ### Arguments
    /// * `admin` - The admin Address
    /// * 'pool_status' - The pool status to be set
    fn set_status(e: Env, admin: Address, pool_status: u32) -> Result<(), PoolError>;

    /// Fetch the status of the pool
    /// * 0 = active
    /// * 1 = on ice
    /// * 2 = frozen
    fn get_status(e: Env) -> u32;

    /// Fetch the name of the pool
    fn get_name(e: Env) -> Symbol;

    /********* Emission Functions **********/

    /// Fetch the next emission configuration
    fn get_emissions_config(e: Env) -> Map<u32, u64>;

    /// Update emissions for reserves for the next emission cycle
    ///
    /// Needs to be performed each emission cycle, as determined by the expiration
    ///
    /// Returns the expiration timestamp
    fn update_emissions(e: Env) -> Result<u64, PoolError>;

    /// Set the emission configuration for the pool
    ///
    /// Changes will be applied in the next pool `update_emissions`, and affect the next emission cycle
    ///
    /// ### Arguments
    /// * `admin` - The Address of the admin
    /// * `res_emission_metadata` - A vector of ReserveEmissionMetadata to update metadata to
    ///
    /// ### Errors
    /// * If the caller is not the admin
    /// * If the sum of ReserveEmissionMetadata shares is greater than 1
    fn set_emissions_config(
        e: Env,
        admin: Address,
        res_emission_metadata: Vec<ReserveEmissionMetadata>,
    ) -> Result<(), PoolError>;

    /// Claims outstanding emissions for the caller for the given reserve's
    ///
    /// Returns the number of tokens claimed
    ///
    /// ### Arguments
    /// * `from` - The address claiming
    /// * `reserve_token_ids` - Vector of reserve token ids
    /// * `to` - The Address to send the claimed tokens to
    fn claim(
        e: Env,
        from: Address,
        reserve_token_ids: Vec<u32>,
        to: Address,
    ) -> Result<i128, PoolError>;

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
    ) -> Result<Option<(ReserveEmissionsConfig, ReserveEmissionsData)>, PoolError>;

    /***** Auction / Liquidation Functions *****/

    /// Creates a new user liquidation auction
    ///
    /// ### Arguments
    /// * `user` - The user getting liquidated through the auction
    /// * `data` - The metadata for the liquidation
    ///
    /// ### Errors
    /// If the user liquidation auction was unable to be created
    fn new_liquidation_auction(
        e: Env,
        user: Address,
        data: LiquidationMetadata,
    ) -> Result<AuctionData, PoolError>;

    /// Delete a user liquidation auction if the user is no longer eligible to be liquidated.
    ///
    /// ### Arguments
    /// * `user` - The user getting liquidated through the auction
    ///
    /// ### Errors
    /// If the user is still eligible to be liquidated state or the auction doesn't exist
    fn del_liquidation_auction(e: Env, user: Address) -> Result<(), PoolError>;

    /// Fetch an auction from the ledger. Returns a quote based on the current block.
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    /// * `user` - The Address involved in the auction
    ///
    /// ### Errors
    /// If the auction does not exist
    fn get_auction(e: Env, auction_type: u32, user: Address) -> Result<AuctionQuote, PoolError>;

    /// Creates a new auction
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    ///
    /// ### Errors
    /// If the auction was unable to be created
    fn new_auction(e: Env, auction_type: u32) -> Result<AuctionData, PoolError>;

    /// Fill the auction from `from`
    ///
    /// Returns the executed AuctionQuote
    ///
    /// ### Arguments
    /// * `from` - The address filling the auction
    /// * `auction_type` - The type of auction
    /// * `user` - The Address involved in the auction
    ///
    /// ### Errors
    /// If the auction does not exist of if the fill action was not successful
    fn fill_auction(
        e: Env,
        from: Address,
        auction_type: u32,
        user: Address,
    ) -> Result<AuctionQuote, PoolError>;
}

#[contractimpl]
impl PoolContractTrait for PoolContract {
    fn initialize(
        e: Env,
        admin: Address,
        name: Symbol,
        oracle: Address,
        bstop_rate: u64,
        backstop_id: Address,
        b_token_hash: BytesN<32>,
        d_token_hash: BytesN<32>,
        blnd_id: Address,
        usdc_id: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();

        pool::execute_initialize(
            &e,
            &admin,
            &name,
            &oracle,
            &bstop_rate,
            &backstop_id,
            &b_token_hash,
            &d_token_hash,
            &blnd_id,
            &usdc_id,
        )
    }

    fn init_reserve(
        e: Env,
        admin: Address,
        asset: Address,
        metadata: ReserveMetadata,
    ) -> Result<(), PoolError> {
        admin.require_auth();

        pool::initialize_reserve(&e, &admin, &asset, &metadata)?;

        e.events()
            .publish((Symbol::new(&e, "init_reserve"), admin), (asset,));
        Ok(())
    }

    fn update_reserve(
        e: Env,
        admin: Address,
        asset: Address,
        metadata: ReserveMetadata,
    ) -> Result<(), PoolError> {
        admin.require_auth();

        pool::execute_update_reserve(&e, &admin, &asset, &metadata)?;

        e.events()
            .publish((Symbol::new(&e, "update_reserve"), admin), (asset,));

        Ok(())
    }

    fn reserve_config(e: Env, asset: Address) -> ReserveConfig {
        storage::get_res_config(&e, &asset)
    }

    // @dev: view
    fn config(e: Env, user: Address) -> u128 {
        storage::get_user_config(&e, &user)
    }

    fn supply(e: Env, from: Address, asset: Address, amount: i128) -> Result<i128, PoolError> {
        from.require_auth();

        let b_tokens_minted = pool::execute_supply(&e, &from, &asset, amount)?;

        e.events().publish(
            (Symbol::new(&e, "supply"), from),
            (asset, amount, b_tokens_minted),
        );

        Ok(b_tokens_minted)
    }

    fn withdraw(
        e: Env,
        from: Address,
        asset: Address,
        amount: i128,
        to: Address,
    ) -> Result<i128, PoolError> {
        from.require_auth();

        let b_tokens_burnt = pool::execute_withdraw(&e, &from, &asset, amount, &to)?;

        e.events().publish(
            (Symbol::new(&e, "withdraw"), from),
            (asset, amount, b_tokens_burnt),
        );

        Ok(b_tokens_burnt)
    }

    fn borrow(
        e: Env,
        from: Address,
        asset: Address,
        amount: i128,
        to: Address,
    ) -> Result<i128, PoolError> {
        from.require_auth();

        let d_tokens_minted = pool::execute_borrow(&e, &from, &asset, amount, &to)?;

        e.events().publish(
            (Symbol::new(&e, "borrow"), from),
            (asset, amount, d_tokens_minted),
        );

        Ok(d_tokens_minted)
    }

    fn repay(
        e: Env,
        from: Address,
        asset: Address,
        amount: i128,
        on_behalf_of: Address,
    ) -> Result<i128, PoolError> {
        from.require_auth();

        let d_tokens_burnt = pool::execute_repay(&e, &from, &asset, amount, &on_behalf_of)?;

        e.events().publish(
            (Symbol::new(&e, "repay"), from),
            (asset, amount, d_tokens_burnt),
        );

        Ok(d_tokens_burnt)
    }

    // TODO: Consolidate functions into universal reserve data view fn
    fn get_d_rate(e: Env, asset: Address) -> i128 {
        let mut res = Reserve::load(&e, asset);
        res.update_rates(&e, storage::get_pool_config(&e).bstop_rate);
        res.data.d_rate
    }

    fn get_b_rate(e: Env, asset: Address) -> i128 {
        let mut res = Reserve::load(&e, asset);
        res.update_rates(&e, storage::get_pool_config(&e).bstop_rate);
        res.get_b_rate(&e)
    }

    fn bad_debt(e: Env, user: Address) -> Result<(), PoolError> {
        bad_debt::manage_bad_debt(&e, &user)
    }

    fn update_state(e: Env) -> Result<u32, PoolError> {
        let new_status = pool::execute_update_pool_status(&e)?;

        // msg.sender
        let caller = e.call_stack().get(0).unwrap().unwrap().0;
        e.events()
            .publish((Symbol::new(&e, "set_status"), caller), new_status);
        Ok(new_status)
    }

    fn set_status(e: Env, admin: Address, pool_status: u32) -> Result<(), PoolError> {
        admin.require_auth();

        pool::set_pool_status(&e, &admin, pool_status)?;

        e.events()
            .publish((Symbol::new(&e, "set_status"), admin), pool_status);
        Ok(())
    }

    // @dev: view
    fn get_status(e: Env) -> u32 {
        storage::get_pool_config(&e).status
    }

    // @dev: view
    fn get_name(e: Env) -> Symbol {
        storage::get_name(&e)
    }

    /********* Emission Functions **********/

    // @dev: view
    fn get_emissions_config(e: Env) -> Map<u32, u64> {
        storage::get_pool_emissions(&e)
    }

    fn update_emissions(e: Env) -> Result<u64, PoolError> {
        let next_expiration = pool::update_pool_emissions(&e)?;

        e.events()
            .publish((Symbol::new(&e, "update_emissions"),), next_expiration);
        Ok(next_expiration)
    }

    fn set_emissions_config(
        e: Env,
        admin: Address,
        res_emission_metadata: Vec<ReserveEmissionMetadata>,
    ) -> Result<(), PoolError> {
        admin.require_auth();

        emissions::set_pool_emissions(&e, res_emission_metadata)
    }

    fn claim(
        e: Env,
        from: Address,
        reserve_token_ids: Vec<u32>,
        to: Address,
    ) -> Result<i128, PoolError> {
        from.require_auth();

        let amount_claimed = emissions::execute_claim(&e, &from, &reserve_token_ids, &to)?;

        e.events().publish(
            (Symbol::new(&e, "claim"), from),
            (reserve_token_ids, amount_claimed),
        );

        Ok(amount_claimed)
    }

    // @dev: view
    fn get_reserve_emissions(
        e: Env,
        asset: Address,
        token_type: u32,
    ) -> Result<Option<(ReserveEmissionsConfig, ReserveEmissionsData)>, PoolError> {
        emissions::get_reserve_emissions(&e, &asset, token_type)
    }

    /***** Auction / Liquidation Functions *****/

    fn new_liquidation_auction(
        e: Env,
        user: Address,
        data: LiquidationMetadata,
    ) -> Result<AuctionData, PoolError> {
        let auction_data = auctions::create_liquidation(&e, &user, data)?;

        e.events().publish(
            (Symbol::new(&e, "new_liquidation_auction"), user),
            auction_data.clone(),
        );

        Ok(auction_data)
    }

    // TODO: Consider checking this before filling an auction based on estimated gas cost.
    fn del_liquidation_auction(e: Env, user: Address) -> Result<(), PoolError> {
        auctions::delete_liquidation(&e, &user)?;

        e.events()
            .publish((Symbol::new(&e, "del_liquidation_auction"), user), ());

        Ok(())
    }

    // @dev: view
    fn get_auction(e: Env, auction_type: u32, user: Address) -> Result<AuctionQuote, PoolError> {
        Ok(auctions::preview_fill(&e, auction_type, &user))
    }

    fn new_auction(e: Env, auction_type: u32) -> Result<AuctionData, PoolError> {
        let auction_data = auctions::create(&e, auction_type)?;

        e.events().publish(
            (Symbol::new(&e, "new_auction"), auction_type),
            auction_data.clone(),
        );

        Ok(auction_data)
    }

    fn fill_auction(
        e: Env,
        from: Address,
        auction_type: u32,
        user: Address,
    ) -> Result<AuctionQuote, PoolError> {
        from.require_auth();

        let auction_quote = auctions::fill(&e, auction_type, &user, &from)?;

        e.events().publish(
            (Symbol::new(&e, "fill_auction"), from),
            (auction_type, user),
        );

        Ok(auction_quote)
    }
}
