use crate::{
    backstop::{self, load_pool_backstop_data, PoolBackstopData, UserBalance, Q4W},
    emissions,
    errors::BackstopError,
    storage,
};
use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Map, Symbol, Vec};

/// ### Backstop Module
///
/// A backstop module for the Blend protocol's Isolated Lending Pools
#[contract]
pub struct BackstopModule;

pub trait BackstopModuleTrait {
    /// Initialize the backstop module
    ///
    /// ### Arguments
    /// * `backstop_token` - The backstop token ID - an LP token with the pair BLND:USDC
    /// * `blnd_token` - The BLND token ID
    /// * `usdc_token` - The USDC token ID
    /// * `pool_factory` - The pool factory ID
    /// * `drop_list` - The list of addresses to distribute initial BLND to and the percent of the distribution they should receive
    ///
    /// ### Errors
    /// If initialize has already been called
    fn initialize(
        e: Env,
        backstop_token: Address,
        blnd_token: Address,
        usdc_token: Address,
        pool_factory: Address,
        drop_list: Map<Address, i128>,
    );

    /********** Core **********/

    /// Deposit backstop tokens from "from" into the backstop of a pool
    ///
    /// Returns the number of backstop pool shares minted
    ///
    /// ### Arguments
    /// * `from` - The address depositing into the backstop
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of tokens to deposit
    fn deposit(e: Env, from: Address, pool_address: Address, amount: i128) -> i128;

    /// Queue deposited pool shares from "from" for withdraw from a backstop of a pool
    ///
    /// Returns the created queue for withdrawal
    ///
    /// ### Arguments
    /// * `from` - The address whose deposits are being queued for withdrawal
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to queue for withdraw
    fn queue_withdrawal(e: Env, from: Address, pool_address: Address, amount: i128) -> Q4W;

    /// Dequeue a currently queued pool share withdraw for "form" from the backstop of a pool
    ///
    /// ### Arguments
    /// * `from` - The address whose deposits are being queued for withdrawal
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to dequeue
    fn dequeue_withdrawal(e: Env, from: Address, pool_address: Address, amount: i128);

    /// Withdraw shares from "from"s withdraw queue for a backstop of a pool
    ///
    /// Returns the amount of tokens returned
    ///
    /// ### Arguments
    /// * `from` - The address whose shares are being withdrawn
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to withdraw
    fn withdraw(e: Env, from: Address, pool_address: Address, amount: i128) -> i128;

    /// Fetch the balance of backstop shares of a pool for the user
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `user` - The user to fetch the balance for
    fn user_balance(e: Env, pool: Address, user: Address) -> UserBalance;

    /// Fetch the backstop data for the pool
    ///
    /// Return a summary of the pool's backstop data
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    fn pool_data(e: Env, pool: Address) -> PoolBackstopData;

    /// Fetch the backstop token for the backstop
    fn backstop_token(e: Env) -> Address;

    /********** Emissions **********/

    /// Update the backstop for the next emissions cycle from the Emitter
    fn update_emission_cycle(e: Env);

    /// Add a pool to the reward zone, and if the reward zone is full, a pool to remove
    ///
    /// ### Arguments
    /// * `to_add` - The address of the pool to add
    /// * `to_remove` - The address of the pool to remove
    ///
    /// ### Errors
    /// If the pool to remove has more tokens, or if distribution occurred in the last 48 hours
    fn add_reward(e: Env, to_add: Address, to_remove: Address);

    /// Fetch the EPS (emissions per second) and expiration for the current distribution window of a pool
    /// in a tuple where (EPS, expiration)
    fn pool_eps(e: Env, pool_address: Address) -> (i128, u64);

    /// Claim backstop deposit emissions from a list of pools for `from`
    ///
    /// Returns the amount of BLND emissions claimed
    ///
    /// ### Arguments
    /// * `from` - The address of the user claiming emissions
    /// * `pool_addresses` - The Vec of addresses to claim backstop deposit emissions from
    /// * `to` - The Address to send to emissions to
    ///
    /// ### Errors
    /// If an invalid pool address is included
    fn claim(e: Env, from: Address, pool_addresses: Vec<Address>, to: Address);

    /// Fetch the drop list
    fn drop_list(e: Env) -> Map<Address, i128>;

    /********** Fund Management *********/

    /// Take backstop token from a pools backstop
    ///
    /// ### Arguments
    /// * `from` - The address of the pool drawing tokens from the backstop
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of backstop tokens to draw
    /// * `to` - The address to send the backstop tokens to
    ///
    /// ### Errors
    /// If the pool does not have enough backstop tokens
    fn draw(e: Env, pool_address: Address, amount: i128, to: Address);

    /// Sends backstop tokens from "from" to a pools backstop
    ///
    /// NOTE: This is not a deposit, and "from" will permanently lose access to the funds
    ///
    /// ### Arguments
    /// * `from` - tge
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of BLND to add
    ///
    /// ### Errors
    /// If the `pool_address` is not valid
    fn donate(e: Env, from: Address, pool_address: Address, amount: i128);

    /// Sends USDC from "from" to a pools backstop to be queued for donation
    ///
    /// NOTE: This is not a deposit, and "from" will permanently lose access to the funds
    ///
    /// ### Arguments
    /// * `from` - tge
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of BLND to add
    ///
    /// ### Errors
    /// If the `pool_address` is not valid
    fn donate_usdc(e: Env, from: Address, pool_address: Address, amount: i128);

    /// Consume donated USDC for a pool and mint LP tokens into the pool's backstop
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    ///
    /// ### Errors
    /// If the `pool_address` is not valid
    fn gulp_usdc(e: Env, pool_address: Address);

    /// Updates the underlying value of 1 backstop token
    ///
    /// ### Returns
    /// A tuple of (blnd_per_tkn, usdc_per_tkn) of underlying value per backstop token
    ///
    /// ### Errors
    /// If the underlying value is unable to be computed
    fn update_tkn_val(e: Env) -> (i128, i128);
}

/// @dev
/// The contract implementation only manages the authorization / authentication required from the caller(s), and
/// utilizes other modules to carry out contract functionality.
#[contractimpl]
impl BackstopModuleTrait for BackstopModule {
    fn initialize(
        e: Env,
        backstop_token: Address,
        usdc_token: Address,
        blnd_token: Address,
        pool_factory: Address,
        drop_list: Map<Address, i128>,
    ) {
        storage::bump_instance(&e);
        if storage::has_backstop_token(&e) {
            panic_with_error!(e, BackstopError::AlreadyInitialized);
        }

        storage::set_backstop_token(&e, &backstop_token);
        storage::set_blnd_token(&e, &blnd_token);
        storage::set_usdc_token(&e, &usdc_token);
        storage::set_pool_factory(&e, &pool_factory);
        storage::set_drop_list(&e, &drop_list);
    }

    /********** Core **********/

    fn deposit(e: Env, from: Address, pool_address: Address, amount: i128) -> i128 {
        storage::bump_instance(&e);
        from.require_auth();

        let to_mint = backstop::execute_deposit(&e, &from, &pool_address, amount);

        e.events().publish(
            (Symbol::new(&e, "deposit"), pool_address, from),
            (amount, to_mint),
        );
        to_mint
    }

    fn queue_withdrawal(e: Env, from: Address, pool_address: Address, amount: i128) -> Q4W {
        storage::bump_instance(&e);
        from.require_auth();

        let to_queue = backstop::execute_queue_withdrawal(&e, &from, &pool_address, amount);

        e.events().publish(
            (Symbol::new(&e, "queue_withdrawal"), pool_address, from),
            (amount, to_queue.exp),
        );
        to_queue
    }

    fn dequeue_withdrawal(e: Env, from: Address, pool_address: Address, amount: i128) {
        storage::bump_instance(&e);
        from.require_auth();

        backstop::execute_dequeue_withdrawal(&e, &from, &pool_address, amount);

        e.events().publish(
            (Symbol::new(&e, "dequeue_withdrawal"), pool_address, from),
            amount,
        );
    }

    fn withdraw(e: Env, from: Address, pool_address: Address, amount: i128) -> i128 {
        storage::bump_instance(&e);
        from.require_auth();

        let to_withdraw = backstop::execute_withdraw(&e, &from, &pool_address, amount);

        e.events().publish(
            (Symbol::new(&e, "withdraw"), pool_address, from),
            (amount, to_withdraw),
        );
        to_withdraw
    }

    fn user_balance(e: Env, pool: Address, user: Address) -> UserBalance {
        storage::get_user_balance(&e, &pool, &user)
    }

    fn pool_data(e: Env, pool: Address) -> PoolBackstopData {
        load_pool_backstop_data(&e, &pool)
    }

    fn backstop_token(e: Env) -> Address {
        storage::get_backstop_token(&e)
    }

    /********** Emissions **********/

    fn update_emission_cycle(e: Env) {
        storage::bump_instance(&e);
        emissions::update_emission_cycle(&e);
    }

    fn add_reward(e: Env, to_add: Address, to_remove: Address) {
        storage::bump_instance(&e);
        emissions::add_to_reward_zone(&e, to_add.clone(), to_remove.clone());

        e.events()
            .publish((Symbol::new(&e, "rw_zone"),), (to_add, to_remove));
    }

    fn pool_eps(e: Env, pool_address: Address) -> (i128, u64) {
        (
            storage::get_pool_eps(&e, &pool_address),
            storage::get_next_emission_cycle(&e),
        )
    }

    fn claim(e: Env, from: Address, pool_addresses: Vec<Address>, to: Address) {
        storage::bump_instance(&e);
        from.require_auth();

        let amount = emissions::execute_claim(&e, &from, &pool_addresses, &to);

        e.events().publish((Symbol::new(&e, "claim"), from), amount);
    }

    fn drop_list(e: Env) -> Map<Address, i128> {
        storage::get_drop_list(&e)
    }

    /********** Fund Management *********/

    fn draw(e: Env, pool_address: Address, amount: i128, to: Address) {
        storage::bump_instance(&e);
        pool_address.require_auth();

        backstop::execute_draw(&e, &pool_address, amount, &to);

        e.events()
            .publish((Symbol::new(&e, "draw"), pool_address), (to, amount));
    }

    fn donate(e: Env, from: Address, pool_address: Address, amount: i128) {
        storage::bump_instance(&e);
        from.require_auth();

        backstop::execute_donate(&e, &from, &pool_address, amount);
        e.events()
            .publish((Symbol::new(&e, "donate"), pool_address, from), amount);
    }

    fn donate_usdc(e: Env, from: Address, pool_address: Address, amount: i128) {
        storage::bump_instance(&e);
        from.require_auth();

        backstop::execute_donate_usdc(&e, &from, &pool_address, amount);
        e.events()
            .publish((Symbol::new(&e, "donate_usdc"), pool_address, from), amount);
    }

    fn gulp_usdc(e: Env, pool_address: Address) {
        storage::bump_instance(&e);

        backstop::execute_gulp_usdc(&e, &pool_address);
        e.events().publish(
            (
                Symbol::new(&e, "gulp_usdc"),
                pool_address,
                e.call_stack().last_unchecked().0,
            ),
            (),
        );
    }

    fn update_tkn_val(e: Env) -> (i128, i128) {
        storage::bump_instance(&e);

        let backstop_token = storage::get_backstop_token(&e);
        let blnd_token = storage::get_blnd_token(&e);
        let usdc_token = storage::get_usdc_token(&e);

        backstop::execute_update_comet_token_value(&e, &backstop_token, &blnd_token, &usdc_token)
    }
}

/// Require that an incoming amount is not negative
///
/// ### Arguments
/// * `amount` - The amount
///
/// ### Errors
/// If the number is negative
pub fn require_nonnegative(e: &Env, amount: i128) {
    if amount.is_negative() {
        panic_with_error!(e, BackstopError::NegativeAmount);
    }
}
