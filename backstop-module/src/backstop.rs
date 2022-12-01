use crate::{
    dependencies::TokenClient,
    errors::BackstopError,
    shares::{to_shares, to_tokens},
    storage::{BackstopDataStore, StorageManager, Q4W},
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

    fn deposit(e: Env, pool: BytesN<32>, amount: u64) -> Result<u64, BackstopError>;

    fn q_withdraw(e: Env, pool: BytesN<32>, amount: u64) -> Result<Q4W, BackstopError>;

    fn withdraw(e: Env, pool: BytesN<32>, amount: u64) -> Result<u64, BackstopError>;

    fn balance(e: Env, pool: BytesN<32>, user: Identifier) -> u64;

    fn q4w(e: Env, pool: BytesN<32>, user: Identifier) -> Vec<Q4W>;

    fn p_balance(e: Env, pool: BytesN<32>) -> (u64, u64, u64);
}

#[contractimpl]
impl BackstopTrait for Backstop {
    fn distribute(e: Env) {
        panic!("not impl")
    }

    fn deposit(e: Env, pool: BytesN<32>, amount: u64) -> Result<u64, BackstopError> {
        let storage = StorageManager::new(&e);
        let invoker_id = Identifier::from(e.invoker());

        // calculate share minting rate
        let pool_shares = storage.get_pool_shares(pool.clone());
        let pool_tokens = storage.get_pool_tokens(pool.clone());
        let to_mint = to_shares(pool_shares, pool_tokens, amount);

        // take tokens from user
        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer_from(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &invoker_id,
            &get_contract_id(&e),
            &BigInt::from_u64(&e, amount),
        );
        storage.set_pool_tokens(pool.clone(), pool_tokens + amount);

        // "mint" shares to the user
        storage.set_pool_shares(pool.clone(), to_mint + pool_shares);
        storage.set_shares(
            pool.clone(),
            invoker_id.clone(),
            storage.get_shares(pool.clone(), invoker_id.clone()) + to_mint,
        );

        // TODO: manage backstop state changes (bToken rates, emissions)

        Ok(to_mint)
    }

    fn q_withdraw(e: Env, pool: BytesN<32>, amount: u64) -> Result<Q4W, BackstopError> {
        let storage = StorageManager::new(&e);
        let invoker_id = Identifier::from(e.invoker());

        let mut user_q4w = storage.get_q4w(pool.clone(), invoker_id.clone());
        let mut user_q4w_amt: u64 = 0;
        for q4w in user_q4w.iter() {
            user_q4w_amt += q4w.unwrap().amount
        }

        let user_shares = storage.get_shares(pool.clone(), invoker_id.clone());
        if user_shares - user_q4w_amt < amount {
            return Err(BackstopError::InvalidBalance);
        }

        // user has enough tokens to withdrawal, add Q4W
        let thirty_days_in_sec = 30 * 24 * 60 * 60;
        let new_q4w = Q4W {
            amount,
            exp: e.ledger().timestamp() + thirty_days_in_sec,
        };
        user_q4w.push_back(new_q4w.clone());
        storage.set_q4w(pool.clone(), invoker_id, user_q4w);

        // reflect changes in pool totals
        storage.set_pool_q4w(pool.clone(), storage.get_pool_q4w(pool.clone()) + amount);

        // TODO: manage backstop state changes (bToken rates)

        Ok(new_q4w)
    }

    fn withdraw(e: Env, pool: BytesN<32>, amount: u64) -> Result<u64, BackstopError> {
        let storage = StorageManager::new(&e);
        let invoker_id = Identifier::from(e.invoker());

        // validate the invoke has enough unlocked Q4W to claim
        // manage the q4w list while verifying
        let mut user_q4w = storage.get_q4w(pool.clone(), invoker_id.clone());
        let mut to_withdraw: u64 = amount;
        for _index in 0..user_q4w.len() {
            let mut cur_q4w = user_q4w.pop_front_unchecked().unwrap();
            if cur_q4w.exp <= e.ledger().timestamp() {
                if cur_q4w.amount > to_withdraw {
                    // last record we need to update, but the q4w should remain
                    cur_q4w.amount -= to_withdraw;
                    to_withdraw = 0;
                    user_q4w.push_front(cur_q4w);
                    break;
                } else if cur_q4w.amount == to_withdraw {
                    // last record we need to update, q4w fully consumed
                    to_withdraw = 0;
                    break;
                } else {
                    // allow the pop to consume the record
                    to_withdraw -= cur_q4w.amount;
                }
            } else {
                return Err(BackstopError::NotExpired);
            }
        }

        if to_withdraw > 0 {
            return Err(BackstopError::InvalidBalance);
        }

        // convert withdrawn shares to tokens
        let pool_shares = storage.get_pool_shares(pool.clone());
        let pool_tokens = storage.get_pool_tokens(pool.clone());
        let to_return = to_tokens(pool_shares, pool_tokens, amount);

        // "burn" shares
        storage.set_pool_shares(pool.clone(), pool_shares - amount);
        storage.set_pool_q4w(pool.clone(), storage.get_pool_q4w(pool.clone()) - amount);
        storage.set_pool_tokens(pool.clone(), pool_tokens - to_return);
        storage.set_shares(
            pool.clone(),
            invoker_id.clone(),
            storage.get_shares(pool.clone(), invoker_id.clone()) - amount,
        );
        storage.set_q4w(pool.clone(), invoker_id.clone(), user_q4w);

        // TODO: manage backstop state changes (emission rates)

        // send tokens back to user
        let blnd_client = TokenClient::new(&e, BytesN::from_array(&e, &BLND_TOKEN));
        blnd_client.xfer(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &invoker_id,
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
