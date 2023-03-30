use crate::{
    backstop, distributor,
    errors::BackstopError,
    storage::{self, Q4W},
};
use soroban_sdk::{contractimpl, symbol, Address, BytesN, Env, Vec};

/// ### Backstop Module
///
/// A backstop module for the Blend protocol's Isolated Lending Pools
pub struct BackstopModuleContract;

pub trait BackstopModuleContractTrait {
    /********** Core **********/

    /// Deposit backstop tokens from "from" into the backstop of a pool
    ///
    /// Returns the number of backstop pool shares minted
    ///
    /// ### Arguments
    /// * `from` - The address depositing into the backstop
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of tokens to deposit
    fn deposit(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<i128, BackstopError>;

    /// Queue deposited pool shares from "from" for withdraw from a backstop of a pool
    ///
    /// Returns the created queue for withdrawal
    ///
    /// ### Arguments
    /// * `from` - The address whose deposits are being queued for withdrawal
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to queue for withdraw
    fn q_withdraw(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<Q4W, BackstopError>;

    /// Dequeue a currently queued pool share withdraw for "form" from the backstop of a pool
    ///
    /// ### Arguments
    /// * `from` - The address whose deposits are being queued for withdrawal
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to dequeue
    fn dequeue_wd(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<(), BackstopError>;

    /// Withdraw shares from "from"s withdraw queue for a backstop of a pool
    ///
    /// Returns the amount of tokens returned
    ///
    /// ### Arguments
    /// * `from` - The address whose shares are being withdrawn
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to withdraw
    fn withdraw(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<i128, BackstopError>;

    /// Fetch the balance of backstop shares of a pool for the user
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `user` - The user to fetch the balance for
    fn balance(e: Env, pool_address: BytesN<32>, user: Address) -> i128;

    /// Fetch the withdraw queue for the user
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `user` - The user to fetch the q4w for
    fn q4w(e: Env, pool_address: BytesN<32>, user: Address) -> Vec<Q4W>;

    /// Fetch the balances for the pool
    ///
    /// Return (total pool backstop tokens, total pool shares, total pool queued for withdraw)
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    fn p_balance(e: Env, pool_address: BytesN<32>) -> (i128, i128, i128);

    /********** Emissions **********/

    /// Distribute BLND from the Emitter
    fn dist(e: Env) -> Result<(), BackstopError>;

    /// Fetch the next distribution window in seconds since epoch in UTC
    fn next_dist(e: Env) -> u64;

    /// Add a pool to the reward zone, and if the reward zone is full, a pool to remove
    ///
    /// ### Arguments
    /// * `to_add` - The address of the pool to add
    /// * `to_remove` - The address of the pool to remove
    ///
    /// ### Errors
    /// If the pool to remove has more tokens, or if distribution occurred in the last 48 hours
    fn add_reward(e: Env, to_add: BytesN<32>, to_remove: BytesN<32>) -> Result<(), BackstopError>;

    /// Fetch the reward zone
    fn get_rz(e: Env) -> Vec<BytesN<32>>;

    /// Fetch the EPS (emissions per second) for the current distribution window of a pool
    fn pool_eps(e: Env, pool_address: BytesN<32>) -> i128;

    /// Allow a pool to claim emissions
    ///
    /// ### Arguments
    /// * `from` - The address of the pool claiming emissions
    /// * `to` - The Address to send to emissions to
    /// * `amount` - The amount of emissions to claim
    ///
    /// ### Errors
    /// If the pool has no emissions left to claim
    /// TODO: Require from == pool_address once sdk issue resolved: https://github.com/stellar/rs-soroban-sdk/issues/868
    fn claim(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        to: Address,
        amount: i128,
    ) -> Result<(), BackstopError>;

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
    /// TODO: Require from == pool_address once sdk issue resolved: https://github.com/stellar/rs-soroban-sdk/issues/868
    fn draw(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
        to: Address,
    ) -> Result<(), BackstopError>;

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
    fn donate(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<(), BackstopError>;
}

/// @dev
/// The contract implementation only manages the authorization / authentication required from the caller(s), and
/// utilizes other modules to carry out contract functionality.
#[contractimpl]
impl BackstopModuleContractTrait for BackstopModuleContract {
    /********** Core **********/

    fn deposit(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<i128, BackstopError> {
        from.require_auth();

        let to_mint = backstop::execute_deposit(&e, &from, &pool_address, amount)?;

        e.events()
            .publish((symbol!("deposit"), pool_address), (from, amount, to_mint));
        Ok(to_mint)
    }

    fn q_withdraw(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<Q4W, BackstopError> {
        from.require_auth();

        let to_queue = backstop::execute_q_withdraw(&e, &from, &pool_address, amount)?;

        e.events().publish(
            (symbol!("q_withdraw"), pool_address),
            (from, amount, to_queue.exp),
        );
        Ok(to_queue)
    }

    fn dequeue_wd(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<(), BackstopError> {
        from.require_auth();

        backstop::execute_dequeue_q4w(&e, &from, &pool_address, amount)?;

        e.events()
            .publish((symbol!("dequeue_wd"), pool_address), (from, amount));
        Ok(())
    }

    fn withdraw(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<i128, BackstopError> {
        from.require_auth();

        let to_withdraw = backstop::execute_withdraw(&e, &from, &pool_address, amount)?;

        e.events().publish(
            (symbol!("withdraw"), pool_address),
            (from, amount, to_withdraw),
        );
        Ok(to_withdraw)
    }

    fn balance(e: Env, pool: BytesN<32>, user: Address) -> i128 {
        storage::get_shares(&e, &pool, &user)
    }

    fn q4w(e: Env, pool: BytesN<32>, user: Address) -> Vec<Q4W> {
        storage::get_q4w(&e, &pool, &user)
    }

    fn p_balance(e: Env, pool: BytesN<32>) -> (i128, i128, i128) {
        (
            storage::get_pool_tokens(&e, &pool),
            storage::get_pool_shares(&e, &pool),
            storage::get_pool_q4w(&e, &pool),
        )
    }

    /********** Emissions **********/

    fn dist(e: Env) -> Result<(), BackstopError> {
        distributor::distribute(&e)?;

        Ok(())
    }

    fn next_dist(e: Env) -> u64 {
        storage::get_next_dist(&e)
    }

    fn add_reward(e: Env, to_add: BytesN<32>, to_remove: BytesN<32>) -> Result<(), BackstopError> {
        distributor::add_to_reward_zone(&e, to_add.clone(), to_remove.clone())?;

        e.events()
            .publish((symbol!("rw_zone"),), (to_add, to_remove));
        Ok(())
    }

    fn get_rz(e: Env) -> Vec<BytesN<32>> {
        storage::get_reward_zone(&e)
    }

    fn pool_eps(e: Env, pool_address: BytesN<32>) -> i128 {
        storage::get_pool_eps(&e, &pool_address)
    }

    fn claim(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        to: Address,
        amount: i128,
    ) -> Result<(), BackstopError> {
        from.require_auth();

        backstop::execute_claim(&e, &pool_address, &to, amount)?;

        e.events()
            .publish((symbol!("claim"), pool_address), (to, amount));
        Ok(())
    }

    /********** Fund Management *********/

    fn draw(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
        to: Address,
    ) -> Result<(), BackstopError> {
        from.require_auth();

        backstop::execute_draw(&e, &pool_address, amount, &to)?;

        e.events()
            .publish((symbol!("draw"), pool_address), (to, amount));
        Ok(())
    }

    fn donate(
        e: Env,
        from: Address,
        pool_address: BytesN<32>,
        amount: i128,
    ) -> Result<(), BackstopError> {
        from.require_auth();

        backstop::execute_donate(&e, &from, &pool_address, amount)?;
        e.events()
            .publish((symbol!("donate"), pool_address), (from, amount));
        Ok(())
    }
}
