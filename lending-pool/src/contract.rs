use crate::{
    auctions::{self, AuctionData, AuctionQuote, LiquidationMetadata},
    bad_debt,
    emissions::{self, ReserveEmissionMetadata},
    errors::PoolError,
    pool,
    storage::{self, ReserveConfig, ReserveEmissionsConfig, ReserveEmissionsData},
};
use soroban_sdk::{contractimpl, symbol, Address, BytesN, Env, Map, Vec};

/// ### Pool
///
/// An isolated money market pool.
pub struct PoolContract;

pub trait PoolContractTrait {
    /// Initialize the pool
    ///
    /// ### Arguments
    /// * `admin` - The Address for the admin
    /// * `oracle` - The contract address of the oracle
    /// * `backstop_id` - The contract address of the pool's backstop module
    /// * `backstop` - TODO: remove once BytesN <-> Address is finished
    /// * `bstop_rate` - The rate of interest shared with the backstop module
    fn initialize(
        e: Env,
        admin: Address,
        oracle: BytesN<32>,
        backstop_id: BytesN<32>,
        backstop: Address,
        bstop_rate: u64,
    );

    /// Initialize a reserve in the pool
    ///
    /// ### Arguments
    /// * `admin` - The Address for the admin
    /// * `asset` - The underlying asset to add as a reserve
    /// * `config` - The ReserveConfig for the reserve
    ///
    /// ### Errors
    /// If the caller is not the admin
    fn init_res(e: Env, admin: Address, asset: BytesN<32>, config: ReserveConfig);

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
    fn supply(e: Env, from: Address, asset: BytesN<32>, amount: i128) -> Result<i128, PoolError>;

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
        asset: BytesN<32>,
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
        asset: BytesN<32>,
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
        asset: BytesN<32>,
        amount: i128,
        on_behalf_of: Address,
    ) -> Result<i128, PoolError>;

    /// Transfer bad debt from a user to the backstop module. Debt is considered "bad" if the user
    /// no longer has any collateral posted. All collateralized reserves for the user must be
    /// liquidated before debt can be transferred to the backstop.
    ///
    /// ### Arguments
    /// * `user` - The user who currently possesses bad debt
    ///
    /// ### Errors
    /// If the user has collateral posted
    fn xfer_bdebt(e: Env, user: Address) -> Result<(), PoolError>;

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
    fn status(e: Env) -> u32;

    /********* Emission Functions **********/

    /// Fetch the next emission configuration
    fn get_emis(e: Env) -> Map<u32, u64>;

    /// Update emissions for reserves for the next emission cycle
    ///
    /// Needs to be performed each emission cycle, as determined by the expiration
    ///
    /// Returns the expiration timestamp
    fn updt_emis(e: Env) -> Result<u64, PoolError>;

    /// Set the emission configuration for the pool
    ///
    /// Changes will be applied in the next pool `updt_emis`, and affect the next emission cycle
    ///
    /// ### Arguments
    /// * `admin` - The Address of the admin
    /// * `res_emission_metadata` - A vector of ReserveEmissionMetadata to update metadata to
    ///
    /// ### Errors
    /// * If the caller is not the admin
    /// * If the sum of ReserveEmissionMetadata shares is greater than 1
    fn set_emis(
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
    fn res_emis(
        e: Env,
        asset: BytesN<32>,
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
    fn new_liq_a(
        e: Env,
        user: Address,
        data: LiquidationMetadata,
    ) -> Result<AuctionData, PoolError>;

    /// Fetch an auction from the ledger. Returns a quote based on the current block.
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    /// * `user` - The Address involved in the auction
    ///
    /// ### Errors
    /// If the auction does not exist
    fn get_auct(e: Env, auction_type: u32, user: Address) -> Result<AuctionQuote, PoolError>;

    /// Creates a new auction
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction
    ///
    /// ### Errors
    /// If the auction was unable to be created
    fn new_auct(e: Env, auction_type: u32) -> Result<AuctionData, PoolError>;

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
    fn fill_auct(
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
        oracle: BytesN<32>,
        backstop_id: BytesN<32>,
        backstop: Address,
        bstop_rate: u64,
    ) {
        admin.require_auth();

        pool::execute_initialize(&e, &admin, &oracle, &backstop_id, &backstop, &bstop_rate);
    }

    fn init_res(e: Env, admin: Address, asset: BytesN<32>, config: ReserveConfig) {
        admin.require_auth();

        pool::initialize_reserve(&e, &admin, &asset, &config);

        e.events().publish(
            (symbol!("init_res"), admin),
            (
                config.b_token.clone(),
                config.d_token.clone(),
                config.decimals,
                config.c_factor,
                config.l_factor,
                config.util,
                config.r_one,
                config.r_two,
                config.r_three,
                config.reactivity,
                config.index,
            ),
        );
    }

    // @dev: view
    fn config(e: Env, user: Address) -> u128 {
        storage::get_user_config(&e, &user)
    }

    fn supply(e: Env, from: Address, asset: BytesN<32>, amount: i128) -> Result<i128, PoolError> {
        from.require_auth();

        let b_tokens_minted = pool::execute_supply(&e, &from, &asset, amount)?;

        e.events()
            .publish((symbol!("supply"), from), (asset, amount, b_tokens_minted));

        Ok(b_tokens_minted)
    }

    fn withdraw(
        e: Env,
        from: Address,
        asset: BytesN<32>,
        amount: i128,
        to: Address,
    ) -> Result<i128, PoolError> {
        from.require_auth();

        let b_tokens_burnt = pool::execute_withdraw(&e, &from, &asset, amount, &to)?;

        e.events()
            .publish((symbol!("withdraw"), from), (asset, amount, b_tokens_burnt));

        Ok(b_tokens_burnt)
    }

    fn borrow(
        e: Env,
        from: Address,
        asset: BytesN<32>,
        amount: i128,
        to: Address,
    ) -> Result<i128, PoolError> {
        from.require_auth();

        let d_tokens_minted = pool::execute_borrow(&e, &from, &asset, amount, &to)?;

        e.events()
            .publish((symbol!("borrow"), from), (asset, amount, d_tokens_minted));

        Ok(d_tokens_minted)
    }

    fn repay(
        e: Env,
        from: Address,
        asset: BytesN<32>,
        amount: i128,
        on_behalf_of: Address,
    ) -> Result<i128, PoolError> {
        from.require_auth();

        let d_tokens_burnt = pool::execute_repay(&e, &from, &asset, amount, &on_behalf_of)?;

        e.events()
            .publish((symbol!("repay"), from), (asset, amount, d_tokens_burnt));

        Ok(d_tokens_burnt)
    }

    fn xfer_bdebt(e: Env, user: Address) -> Result<(), PoolError> {
        bad_debt::transfer_bad_debt_to_backstop(&e, &user)
    }

    fn set_status(e: Env, admin: Address, pool_status: u32) -> Result<(), PoolError> {
        admin.require_auth();

        pool::set_pool_status(&e, &admin, pool_status)?;

        e.events()
            .publish((symbol!("set_status"), admin), pool_status);
        Ok(())
    }

    // @dev: view
    fn status(e: Env) -> u32 {
        storage::get_pool_config(&e).status
    }

    /********* Emission Functions **********/

    // @dev: view
    fn get_emis(e: Env) -> Map<u32, u64> {
        storage::get_pool_emissions(&e)
    }

    fn updt_emis(e: Env) -> Result<u64, PoolError> {
        let next_expiration = pool::update_pool_emissions(&e)?;

        e.events().publish((symbol!("updt_emis"),), next_expiration);
        Ok(next_expiration)
    }

    fn set_emis(
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
        let amount_claimed = emissions::execute_claim(&e, &from, &reserve_token_ids, &to)?;

        e.events().publish(
            (symbol!("claim"), from),
            (reserve_token_ids, amount_claimed),
        );

        Ok(amount_claimed)
    }

    // @dev: view
    fn res_emis(
        e: Env,
        asset: BytesN<32>,
        token_type: u32,
    ) -> Result<Option<(ReserveEmissionsConfig, ReserveEmissionsData)>, PoolError> {
        emissions::get_reserve_emissions(&e, &asset, token_type)
    }

    /***** Auction / Liquidation Functions *****/

    fn new_liq_a(
        e: Env,
        user: Address,
        data: LiquidationMetadata,
    ) -> Result<AuctionData, PoolError> {
        let auction_data = auctions::create_liquidation(&e, &user, data)?;

        e.events()
            .publish((symbol!("new_liq_a"), user), auction_data.clone());

        Ok(auction_data)
    }

    // @dev: view
    fn get_auct(e: Env, auction_type: u32, user: Address) -> Result<AuctionQuote, PoolError> {
        Ok(auctions::preview_fill(&e, auction_type, &user))
    }

    fn new_auct(e: Env, auction_type: u32) -> Result<AuctionData, PoolError> {
        let auction_data = auctions::create(&e, auction_type)?;

        e.events()
            .publish((symbol!("new_auct"), auction_type), auction_data.clone());

        Ok(auction_data)
    }

    fn fill_auct(
        e: Env,
        from: Address,
        auction_type: u32,
        user: Address,
    ) -> Result<AuctionQuote, PoolError> {
        let auction_quote = auctions::fill(&e, auction_type, &user, &from)?;

        e.events()
            .publish((symbol!("fill_auct"), from), (auction_type, user));

        Ok(auction_quote)
    }
}
