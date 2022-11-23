use crate::{
    dependencies::TokenClient,
    errors::EmitterError,
    lp_reader::get_lp_blend_holdings,
    storage::{EmitterDataKey, EmitterDataStore, StorageManager},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, BigInt, BytesN, Env};

const SCALAR: i64 = 1_000_000_0;

/// ### Pool
///
/// An isolated money market pool.
pub struct Emitter;

pub trait EmitterTrait {
    /// Initialize the Emitter
    ///
    /// ### Arguments
    /// * `backstop` - The backstop module address
    /// * `blend_id` - The address of the blend token
    /// * `blend_lp_id` - The contract address of the blend LP token contract
    fn initialize(e: Env, backstop: Identifier, blend_id: BytesN<32>, blend_lp_id: Identifier);

    /// Distributes BLEND tokens to the listed backstop module
    ///
    /// Returns the amount of BLEND tokens distributed
    ///
    /// ### Arguments
    ///
    /// ### Errors
    /// If the caller is not the listed backstop module
    fn distribute(e: Env) -> Result<BigInt, EmitterError>;

    /// Switches the listed backstop module to one with more effective backstop deposits
    ///
    /// Returns OK or an error
    ///
    /// ### Arguments
    /// * `new_backstop` - The contract address of the new backstop module
    ///
    /// ### Errors
    /// If the input contract does not have more backstop deposits than the listed backstop module
    fn swap_bstop(e: Env, new_backstop: Identifier) -> Result<(), EmitterError>;
}

#[contractimpl]
impl EmitterTrait for Emitter {
    fn initialize(e: Env, backstop: Identifier, blend_id: BytesN<32>, blend_lp_id: Identifier) {
        let storage = StorageManager::new(&e);
        if storage.env().data().has(EmitterDataKey::Backstop) {
            panic!("Emitter already initialized");
        }
        storage.set_backstop_id(backstop);
        storage.set_blend_id(blend_id);
        storage.set_blend_lp_id(blend_lp_id);
        storage.set_last_distro_time(e.ledger().timestamp());
    }

    fn distribute(e: Env) -> Result<BigInt, EmitterError> {
        let storage = StorageManager::new(&e);
        let backstop = storage.get_backstop_id();
        if backstop != Identifier::from(e.invoker()) {
            return Err(EmitterError::NotAuthorized);
        }
        let timestamp = e.ledger().timestamp();
        let seconds_since_last_distro =
            BigInt::from_u64(&e, timestamp - storage.get_last_distro_time());
        let distribution_amount = seconds_since_last_distro * BigInt::from_i64(&e, SCALAR);

        let blend_client = get_blend_token_client(&e, &storage);
        blend_client.xfer(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &storage.get_backstop_id(),
            &distribution_amount,
        );
        Ok(distribution_amount)
    }

    fn swap_bstop(e: Env, new_backstop: Identifier) -> Result<(), EmitterError> {
        let storage = StorageManager::new(&e);
        let blend_client = get_blend_token_client(&e, &storage);

        let old_backstop = storage.get_backstop_id();
        let old_backstop_blend_balance = blend_client.balance(&old_backstop);
        let old_backstop_blend_lp_balance = get_lp_blend_holdings(&e, old_backstop);
        let effective_old_backstop_blend =
            old_backstop_blend_balance / 4 + old_backstop_blend_lp_balance;

        let new_backstop_blend_balance = blend_client.balance(&new_backstop);
        let new_backstop_blend_lp_balance = get_lp_blend_holdings(&e, new_backstop.clone());
        let effective_new_backstop_blend =
            new_backstop_blend_balance / 4 + new_backstop_blend_lp_balance;

        if effective_new_backstop_blend <= effective_old_backstop_blend {
            return Err(EmitterError::InsufficientBLND);
        }

        storage.set_backstop_id(new_backstop);
        Ok(())
    }
}

// ****** Helpers ********

pub fn get_blend_token_client(e: &Env, storage: &StorageManager) -> TokenClient {
    let id = storage.get_blend_id();
    TokenClient::new(e, id)
}
