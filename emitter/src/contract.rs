use crate::{emitter, errors::EmitterError, storage};
use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Symbol};

/// ### Emitter
///
/// Emits Blend tokens to the backstop module
#[contract]
pub struct EmitterContract;

pub trait EmitterContractTrait {
    /// Initialize the Emitter
    ///
    /// ### Arguments
    /// * `backstop_id` - The backstop module Address ID
    /// * `blnd_token_id` - The Blend token Address ID
    fn initialize(e: Env, backstop: Address, blnd_token_id: Address);

    /// Distributes BLND tokens to the listed backstop module
    ///
    /// Returns the amount of BLND tokens distributed
    ///
    /// ### Errors
    /// If the caller is not the listed backstop module
    fn distribute(e: Env) -> Result<i128, EmitterError>;

    /// Fetch the current backstop
    fn get_backstop(e: Env) -> Address;

    /// Switches the listed backstop module to one with more effective backstop deposits
    ///
    /// Returns OK or an error
    ///
    /// ### Arguments
    /// * `new_backstop_id` - The Address ID of the new backstop module
    ///
    /// ### Errors
    /// If the input contract does not have more backstop deposits than the listed backstop module
    fn swap_backstop(e: Env, new_backstop_id: Address);
}

#[contractimpl]
impl EmitterContractTrait for EmitterContract {
    fn initialize(e: Env, backstop: Address, blnd_token_id: Address) {
        if storage::has_backstop(&e) {
            panic_with_error!(&e, EmitterError::AlreadyInitialized)
        }

        storage::set_backstop(&e, &backstop);
        storage::set_blend_id(&e, &blnd_token_id);
        // TODO: Determine if setting the last distro time here is appropriate, since it means tokens immediately start being distributed
        storage::set_last_distro_time(&e, &(e.ledger().timestamp() - 7 * 24 * 60 * 60));
    }

    fn distribute(e: Env) -> Result<i128, EmitterError> {
        let backstop_address = storage::get_backstop(&e);
        backstop_address.require_auth();

        let distribution_amount = emitter::execute_distribute(&e, &backstop_address)?;

        e.events().publish(
            (Symbol::new(&e, "distribute"),),
            (backstop_address, distribution_amount),
        );
        Ok(distribution_amount)
    }

    fn get_backstop(e: Env) -> Address {
        storage::get_backstop(&e)
    }

    fn swap_backstop(e: Env, new_backstop_id: Address) {
        emitter::execute_swap_backstop(&e, new_backstop_id.clone());

        e.events()
            .publish((Symbol::new(&e, "swap"),), (new_backstop_id,));
    }
}
