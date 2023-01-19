use crate::{
    dependencies::TokenClient,
    errors::EmitterError,
    lp_reader::get_lp_blend_holdings,
    storage::{EmitterDataStore, StorageManager},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, BytesN, Env};

const STROOP: u64 = 1_0000000;

/// ### Emitter
///
/// Emits Blend tokens to the backstop module
pub struct Emitter;

pub trait EmitterTrait {
    /// Initialize the Emitter
    ///
    /// ### Arguments
    /// * `backstop` - The backstop module address
    /// * `blend_id` - The address of the blend token
    /// * `blend_lp_id` - The contract address of the blend LP token contract
    fn initialize(e: Env, backstop: BytesN<32>, blend_id: BytesN<32>, blend_lp_id: BytesN<32>);

    /// Distributes BLEND tokens to the listed backstop module
    ///
    /// Returns the amount of BLEND tokens distributed
    ///
    /// ### Errors
    /// If the caller is not the listed backstop module
    fn distribute(e: Env) -> Result<u64, EmitterError>;

    /// Fetch the current backstop
    fn get_bstop(e: Env) -> BytesN<32>;

    /// Switches the listed backstop module to one with more effective backstop deposits
    ///
    /// Returns OK or an error
    ///
    /// ### Arguments
    /// * `new_backstop` - The contract address of the new backstop module
    ///
    /// ### Errors
    /// If the input contract does not have more backstop deposits than the listed backstop module
    fn swap_bstop(e: Env, new_backstop: BytesN<32>) -> Result<(), EmitterError>;
}

#[contractimpl]
impl EmitterTrait for Emitter {
    fn initialize(e: Env, backstop: BytesN<32>, blend_id: BytesN<32>, blend_lp_id: BytesN<32>) {
        let storage = StorageManager::new(&e);
        if storage.is_backstop_set() {
            panic!("Emitter already initialized");
        }
        storage.set_backstop(backstop);
        storage.set_blend_id(blend_id);
        storage.set_blend_lp_id(blend_lp_id);
        //TODO: Determine if setting the last distro time here is appropriate, since it means tokens immediately start being distributed
        storage.set_last_distro_time(e.ledger().timestamp());
    }

    fn distribute(e: Env) -> Result<u64, EmitterError> {
        let storage = StorageManager::new(&e);
        let backstop = Identifier::Contract(storage.get_backstop());
        if backstop != Identifier::from(e.invoker()) {
            return Err(EmitterError::NotAuthorized);
        }
        
        let timestamp = e.ledger().timestamp();
        let seconds_since_last_distro = timestamp - storage.get_last_distro_time();
        // Blend tokens are distributed at a rate of 1 token per second
        let distribution_amount = seconds_since_last_distro * STROOP;

        let blend_client = get_blend_token_client(&e, &storage);
        blend_client.mint(
            &Signature::Invoker,
            &0,
            &backstop,
            &(distribution_amount as i128),
        );
        Ok(distribution_amount)
    }

    fn get_bstop(e: Env) -> BytesN<32> {
        let storage = StorageManager::new(&e);
        storage.get_backstop()
    }

    fn swap_bstop(e: Env, new_backstop: BytesN<32>) -> Result<(), EmitterError> {
        let storage = StorageManager::new(&e);
        let blend_client = get_blend_token_client(&e, &storage);
        let new_backstop_id = Identifier::Contract(new_backstop.clone());

        let old_backstop = Identifier::Contract(storage.get_backstop());
        let old_backstop_blend_balance = blend_client.balance(&old_backstop);
        let old_backstop_blend_lp_balance = get_lp_blend_holdings(&e, old_backstop);
        let effective_old_backstop_blend =
            (old_backstop_blend_balance / 4) + old_backstop_blend_lp_balance;

        let new_backstop_blend_balance = blend_client.balance(&new_backstop_id);
        let new_backstop_blend_lp_balance = get_lp_blend_holdings(&e, new_backstop_id.clone());
        let effective_new_backstop_blend =
            (new_backstop_blend_balance / 4) + new_backstop_blend_lp_balance;

        if effective_new_backstop_blend <= effective_old_backstop_blend {
            return Err(EmitterError::InsufficientBLND);
        }

        storage.set_backstop(new_backstop);
        Ok(())
    }
}

// ****** Helpers ********

pub fn get_blend_token_client(e: &Env, storage: &StorageManager) -> TokenClient {
    let id = storage.get_blend_id();
    TokenClient::new(e, id)
}
