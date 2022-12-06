use crate::{
    dependencies::TokenClient,
    errors::BackstopError,
    pool::Pool,
    storage::{BackstopDataStore, StorageManager, Q4W},
    user::User,
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, BigInt, BytesN, Env, Vec};

/// ### Backstop Module
///
/// A backstop module for the Blend protocol's Isolated Lending Pools
pub struct Backstop;

const BLND_TOKEN: [u8; 32] = [222; 32]; // TODO: Use actual token bytes

pub trait BackstopTrait {
    fn distribute(e: Env);

    fn deposit(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError>;

    fn q_withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<Q4W, BackstopError>;

    fn withdraw(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError>;

    fn balance(e: Env, pool_address: BytesN<32>, user: Identifier) -> u64;

    fn q4w(e: Env, pool_address: BytesN<32>, user: Identifier) -> Vec<Q4W>;

    fn p_balance(e: Env, pool_address: BytesN<32>) -> (u64, u64, u64);
}

#[contractimpl]
impl BackstopTrait for Backstop {
    fn distribute(_e: Env) {
        panic!("not impl")
    }

    fn deposit(e: Env, pool_address: BytesN<32>, amount: u64) -> Result<u64, BackstopError> {
        let mut user = User::new(pool_address.clone(), Identifier::from(e.invoker()));
        let mut pool = Pool::new(pool_address);

        // calculate share minting rate
        let to_mint = pool.convert_to_shares(&e, amount);

        // take tokens from user
        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer_from(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &user.id,
            &get_contract_id(&e),
            &BigInt::from_u64(&e, amount),
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
        blnd_client.xfer(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &user.id,
            &BigInt::from_u64(&e, to_return),
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
}

// ***** Helpers *****

fn get_contract_id(e: &Env) -> Identifier {
    Identifier::Contract(e.current_contract())
}
