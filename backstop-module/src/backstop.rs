use crate::{
    dependencies::TokenClient,
    distributor::Distributor,
    errors::BackstopError,
    pool::Pool,
    storage::{BackstopDataStore, StorageManager, Q4W},
    user::User,
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, BytesN, Env, Vec};

/// ### Backstop Module
///
/// A backstop module for the Blend protocol's Isolated Lending Pools
pub struct Backstop;

const BLND_TOKEN: [u8; 32] = [222; 32]; // TODO: Use actual token bytes

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

    /********** Fund Management *********/

    /// Take BLND from a pools backstop and update share value
    ///
    /// ### Arguments
    /// * `pool_address` - The address of the pool
    /// * `amount` - The amount of BLND to take
    /// * `to` - The address to send the BLND to
    ///
    /// ### Errors
    /// If the pool does not have enough BLND
    /// If the function is invoked by something other than the specified pool
    fn draw(
        e: Env,
        pool_address: BytesN<32>,
        amount: u64,
        to: Identifier,
    ) -> Result<(), BackstopError>;

    //add BLND to a pools backstop and update share value
    fn donate(
        e: Env,
        pool_address: BytesN<32>,
        amount: u64,
        from: Identifier,
    ) -> Result<(), BackstopError>;
}

#[contractimpl]
impl BackstopTrait for Backstop {
    fn deposit(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError> {
        let mut user = User::new(pool_address.clone(), Identifier::from(e.invoker()));
        let mut pool = Pool::new(pool_address);

        // calculate share minting rate
        let to_mint = pool.convert_to_shares(&e, amount);

        // take tokens from user
        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer_from(
            &Signature::Invoker,
            &0,
            &user.id,
            &get_contract_id(&e),
            &(amount as i128),
        );

        // "mint" shares to the user
        // TODO: storing and writing are currently separated. Consider revisiting
        // after the logic for emissions and interest rates is added
        pool.deposit(&e, amount, to_mint);
        pool.write_shares(&e);
        pool.write_tokens(&e);

        user.add_shares(&e, to_mint);
        user.write_shares(&e);

        // TODO: manage backstop state changes (bToken rates, emissions)

        Ok(to_mint)
    }

    fn q_withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<Q4W, BackstopError> {
        let mut user = User::new(pool_address.clone(), Identifier::from(e.invoker()));
        let mut pool = Pool::new(pool_address);

        let new_q4w = match user.try_queue_shares_for_withdrawal(&e, amount) {
            Ok(q4w) => q4w,
            Err(e) => return Err(e),
        };
        user.write_q4w(&e);

        pool.queue_for_withdraw(&e, amount);
        pool.write_q4w(&e);

        // TODO: manage backstop state changes (bToken rates)

        Ok(new_q4w)
    }

    fn withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError> {
        let mut user = User::new(pool_address.clone(), Identifier::from(e.invoker()));
        let mut pool = Pool::new(pool_address);

        match user.try_withdraw_shares(&e, amount) {
            Ok(_) => (),
            Err(e) => return Err(e),
        };

        // convert withdrawn shares to tokens
        let to_return = pool.convert_to_tokens(&e, amount);

        // "burn" shares
        pool.withdraw(&e, to_return, amount);
        pool.write_shares(&e);
        pool.write_tokens(&e);
        pool.write_q4w(&e);

        user.write_q4w(&e);
        user.write_shares(&e);

        // TODO: manage backstop state changes (emission rates)

        // send tokens back to user
        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer(&Signature::Invoker, &0, &user.id, &(to_return as i128));

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

    /********** Fund Management *********/

    fn draw(
        e: Env,
        pool_address: BytesN<32>,
        amount: u64,
        to: Identifier,
    ) -> Result<(), BackstopError> {
        //only pool can draw
        if Identifier::Contract(pool_address.clone()) != Identifier::from(e.invoker()) {
            return Err(BackstopError::Unauthorized);
        }
        let mut pool = Pool::new(pool_address);

        // update pool state
        if pool.get_tokens(&e) < amount {
            return Err(BackstopError::InsufficientFunds);
        }
        pool.withdraw(&e, amount, 0);
        pool.write_tokens(&e);

        // send tokens to recipient
        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer(&Signature::Invoker, &0, &to, &(amount as i128));

        Ok(())
    }

    fn donate(
        e: Env,
        pool_address: BytesN<32>,
        amount: u64,
        from: Identifier,
    ) -> Result<(), BackstopError> {
        //only pool can donate
        if Identifier::Contract(pool_address.clone()) != Identifier::from(e.invoker()) {
            return Err(BackstopError::Unauthorized);
        }
        // send tokens to recipient
        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer_from(
            &Signature::Invoker,
            &0,
            &from,
            &Identifier::Contract(e.current_contract()),
            &(amount as i128),
        );
        // update backstop state
        let mut pool = Pool::new(pool_address);
        pool.deposit(&e, amount, 0);
        pool.write_tokens(&e);
        Ok(())
    }
}

// ***** Helpers *****

fn get_contract_id(e: &Env) -> Identifier {
    Identifier::Contract(e.current_contract())
}
