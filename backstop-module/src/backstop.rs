use crate::{
    constants::BLND_TOKEN,
    dependencies::TokenClient,
    distributor::Distributor,
    errors::BackstopError,
    pool::Pool,
    storage::{BackstopDataStore, StorageManager, Q4W},
    user::User,
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, symbol, Address, BytesN, Env, Vec};
use cast::i128;


/// ### Backstop Module
///
/// A backstop module for the Blend protocol's Isolated Lending Pools
pub struct Backstop;

pub trait BackstopTrait {
    /********** Core **********/

    /// Deposit tokens from the invoker into the backstop of a pool
    ///
    /// Returns the number of backstop pool shares minted
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of tokens to deposit
    fn deposit(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError>;

    /// Queue deposited pool shares for withdraw from a backstop of a pool
    ///
    /// Returns the created queue for withdrawal
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to queue for withdraw
    fn q_withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<Q4W, BackstopError>;

    /// Withdraw shares from the withdraw queue for a backstop of a pool
    ///
    /// Returns the amount of tokens returned
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of shares to withdraw
    fn withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError>;

    /// Fetch the balance of backstop shares of a pool for the user
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `user` - The user to fetch the balance for
    fn balance(e: Env, pool_address: BytesN<32>, user: Identifier) -> u64;

    /// Fetch the withdraw queue for the user
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `user` - The user to fetch the q4w for
    fn q4w(e: Env, pool_address: BytesN<32>, user: Identifier) -> Vec<Q4W>;

    /// Fetch the balances for the pool
    ///
    /// Return (total pool backstop tokens, total pool shares, total pool queued for withdraw)
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    fn p_balance(e: Env, pool_address: BytesN<32>) -> (u64, u64, u64);

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
    fn pool_eps(e: Env, pool_address: BytesN<32>) -> u64;

    /// Allow a pool to claim emissions
    ///
    /// ### Arguments
    /// * `to` - The identifier to send to emissions to
    /// * `amount` - The amount of emissions to claim
    ///
    /// ### Errors
    /// If the pool has no emissions left to claim
    fn claim(e: Env, to: Identifier, amount: u64) -> Result<(), BackstopError>;

    /********** Fund Management *********/

    /// Take BLND from a pools backstop and update share value
    ///
    /// ### Arguments
    /// * `amount` - The amount of BLND to take
    /// * `to` - The address to send the BLND to
    ///
    /// ### Errors
    /// If the pool does not have enough BLND
    /// If the function is not invoked by a valid pool
    fn draw(e: Env, amount: u64, to: Identifier) -> Result<(), BackstopError>;

    /// Sends BLND from the invoker to a pools backstop and update share value
    ///
    /// NOTE: This is not a deposit, and the invoker will permanently lose access to the funds
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of BLND to add
    ///
    /// ### Errors
    /// If the `pool_address` is not valid
    fn donate(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<(), BackstopError>;
}

#[contractimpl]
impl BackstopTrait for Backstop {
    fn deposit(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError> {
        let mut user = User::new(pool_address.clone(), Identifier::from(e.invoker()));
        let mut pool = Pool::new(pool_address.clone());

        let to_mint = pool.convert_to_shares(&e, amount);

        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer_from(
            &Signature::Invoker,
            &0,
            &user.id,
            &get_contract_id(&e),
            &i128(amount),
        );

        // "mint" shares
        // TODO: storing and writing are currently separated. Consider revisiting
        // after the logic for emissions and interest rates is added
        pool.deposit(&e, amount, to_mint);
        pool.write_shares(&e);
        pool.write_tokens(&e);

        user.add_shares(&e, to_mint);
        user.write_shares(&e);

        e.events().publish(
            (symbol!("Backstop"), symbol!("Deposit")),
            (pool_address, user.id, amount),
        );
        Ok(to_mint)
    }

    fn q_withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<Q4W, BackstopError> {
        let mut user = User::new(pool_address.clone(), Identifier::from(e.invoker()));
        let mut pool = Pool::new(pool_address.clone());

        let new_q4w = user.try_queue_shares_for_withdrawal(&e, amount)?;

        user.write_q4w(&e);

        pool.queue_for_withdraw(&e, amount);
        pool.write_q4w(&e);

        e.events().publish(
            (symbol!("Backstop"), symbol!("Queue"), symbol!("Withdraw")),
            (pool_address, user.id, amount),
        );
        Ok(new_q4w)
    }

    fn withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError> {
        let mut user = User::new(pool_address.clone(), Identifier::from(e.invoker()));
        let mut pool = Pool::new(pool_address.clone());

        user.try_withdraw_shares(&e, amount)?;

        let to_return = pool.convert_to_tokens(&e, amount);

        // "burn" shares
        pool.withdraw(&e, to_return, amount)?;
        pool.write_shares(&e);
        pool.write_tokens(&e);
        pool.write_q4w(&e);

        user.write_q4w(&e);
        user.write_shares(&e);

        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer(&Signature::Invoker, &0, &user.id, &i128(to_return));

        e.events().publish(
            (symbol!("Backstop"), symbol!("Withdraw")),
            (pool_address, user.id, to_return),
        );
        Ok(to_return)
    }

    fn balance(e: Env, pool: BytesN<32>, user: Identifier) -> u64 {
        let storage = StorageManager::new(&e);
        storage.get_shares(pool, user)
    }

    fn q4w(e: Env, pool: BytesN<32>, user: Identifier) -> Vec<Q4W> {
        let storage = StorageManager::new(&e);
        storage.get_q4w(pool, user)
    }

    fn p_balance(e: Env, pool: BytesN<32>) -> (u64, u64, u64) {
        let storage = StorageManager::new(&e);
        let pool_tokens = storage.get_pool_tokens(pool.clone());
        let pool_shares = storage.get_pool_shares(pool.clone());
        let pool_q4w = storage.get_pool_q4w(pool.clone());
        (pool_tokens, pool_shares, pool_q4w)
    }

    /********** Emissions **********/

    fn dist(e: Env) -> Result<(), BackstopError> {
        Distributor::distribute(&e)
    }

    fn next_dist(e: Env) -> u64 {
        StorageManager::new(&e).get_next_dist()
    }

    fn add_reward(e: Env, to_add: BytesN<32>, to_remove: BytesN<32>) -> Result<(), BackstopError> {
        Distributor::add_to_reward_zone(&e, to_add, to_remove)
    }

    fn get_rz(e: Env) -> Vec<BytesN<32>> {
        StorageManager::new(&e).get_reward_zone()
    }

    fn pool_eps(e: Env, pool_address: BytesN<32>) -> u64 {
        StorageManager::new(&e).get_pool_eps(pool_address)
    }

    fn claim(e: Env, to: Identifier, amount: u64) -> Result<(), BackstopError> {
        let mut pool = match e.invoker() {
            Address::Contract(invoker) => Pool::new(invoker),
            _ => return Err(BackstopError::NotPool),
        };
        pool.verify_pool(&e)?;
        pool.claim(&e, amount)?;
        pool.write_emissions(&e);

        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer(&Signature::Invoker, &0, &to, &i128(amount));

        Ok(())
    }

    /********** Fund Management *********/

    fn draw(e: Env, amount: u64, to: Identifier) -> Result<(), BackstopError> {
        let mut pool = match e.invoker() {
            Address::Contract(invoker) => Pool::new(invoker),
            _ => return Err(BackstopError::NotPool),
        };
        pool.verify_pool(&e)?;

        pool.withdraw(&e, amount, 0)?;
        pool.write_tokens(&e);

        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer(&Signature::Invoker, &0, &to, &i128(amount));

        Ok(())
    }

    fn donate(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<(), BackstopError> {
        let mut pool = Pool::new(pool_address);

        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer_from(
            &Signature::Invoker,
            &0,
            &Identifier::from(e.invoker()),
            &get_contract_id(&e),
            &i128(amount),
        );

        pool.deposit(&e, amount, 0);
        pool.write_tokens(&e);

        Ok(())
    }
}

// ***** Helpers *****

fn get_contract_id(e: &Env) -> Identifier {
    Identifier::Contract(e.current_contract())
}
