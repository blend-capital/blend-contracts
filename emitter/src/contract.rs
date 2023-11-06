use crate::{emitter, errors::EmitterError, storage};
use soroban_sdk::{contract, contractclient, contractimpl, panic_with_error, Address, Env, Symbol};

/// ### Emitter
///
/// Emits Blend tokens to the backstop module
#[contract]
pub struct EmitterContract;

#[contractclient(name = "EmitterClient")]
pub trait Emitter {
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
    fn distribute(e: Env) -> i128;

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

    /// Distributes initial BLND post-backstop swap or protocol launch
    ///
    /// Returns OK or an error
    ///
    /// ### Errors
    /// If drop has already been called for this backstop
    fn drop(e: Env);
}

#[contractimpl]
impl Emitter for EmitterContract {
    fn initialize(e: Env, backstop: Address, blnd_token_id: Address) {
        storage::bump_instance(&e);
        if storage::has_backstop(&e) {
            panic_with_error!(&e, EmitterError::AlreadyInitialized)
        }

        storage::set_backstop(&e, &backstop);
        storage::set_blend_id(&e, &blnd_token_id);
        storage::set_last_fork(&e, 0); // We set the block 45 days in the past to allow for an immediate initial drop
        storage::set_drop_status(&e, false);
        // TODO: Determine if setting the last distro time here is appropriate, since it means tokens immediately start being distributed
        storage::set_last_distro_time(&e, &(e.ledger().timestamp() - 7 * 24 * 60 * 60));
    }

    fn distribute(e: Env) -> i128 {
        storage::bump_instance(&e);
        let backstop_address = storage::get_backstop(&e);

        let distribution_amount = emitter::execute_distribute(&e, &backstop_address);

        e.events().publish(
            (Symbol::new(&e, "distribute"),),
            (backstop_address, distribution_amount),
        );
        distribution_amount
    }

    fn get_backstop(e: Env) -> Address {
        storage::get_backstop(&e)
    }

    fn swap_backstop(e: Env, new_backstop_id: Address) {
        storage::bump_instance(&e);
        emitter::execute_swap_backstop(&e, new_backstop_id.clone());

        e.events()
            .publish((Symbol::new(&e, "swap"),), (new_backstop_id,));
    }

    fn drop(e: Env) {
        storage::bump_instance(&e);
        let drop_list = emitter::execute_drop(&e);

        e.events().publish((Symbol::new(&e, "drop"),), drop_list);
    }
}
