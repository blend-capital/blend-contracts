use crate::{backstop_manager, emitter, errors::EmitterError, storage};
use soroban_sdk::{
    contract, contractclient, contractimpl, panic_with_error, Address, Env, Map, Symbol,
};

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
    /// * `blnd_token` - The Blend token Address the Emitter will distribute
    /// * `backstop` - The backstop module address to emit to
    /// * `backstop_token` - The token the backstop takes deposits in
    fn initialize(e: Env, blnd_token: Address, backstop: Address, backstop_token: Address);

    /// Distributes BLND tokens to the listed backstop module
    ///
    /// Returns the amount of BLND tokens distributed
    fn distribute(e: Env) -> i128;

    /// Fetch the last time the Emitter distributed to the backstop module
    ///
    /// ### Arguments
    /// * `backstop` - The backstop module Address ID
    fn get_last_distro(e: Env, backstop_id: Address) -> u64;

    /// Fetch the current backstop
    fn get_backstop(e: Env) -> Address;

    /// Queues up a swap of the listed backstop module and token to new addresses.
    ///
    /// ### Arguments
    /// * `new_backstop` - The Address of the new backstop module
    /// * `new_backstop_token` - The address of the new backstop token
    ///
    /// ### Errors
    /// If the input contract does not have more backstop deposits than the listed backstop module of the
    /// current backstop token.
    fn queue_swap_backstop(e: Env, new_backstop: Address, new_backstop_token: Address);

    /// Fetch the queued backstop swap, or None if nothing is queued.
    fn get_queued_swap(e: Env) -> Option<backstop_manager::Swap>;

    /// Verifies that a queued swap still meets the requirements to be executed. If not,
    /// the queued swap is cancelled and must be recreated.
    ///
    /// ### Errors
    /// If the queued swap is still valid.
    fn cancel_swap_backstop(e: Env);

    /// Executes a queued swap of the listed backstop module to one with more effective backstop deposits
    ///
    /// ### Errors
    /// If the input contract does not have more backstop deposits than the listed backstop module,
    /// or if the queued swap has not been unlocked.
    fn swap_backstop(e: Env);

    /// Distributes initial BLND after a new backstop is set
    ///
    /// ### Arguments
    /// * `list` - The list of address and amounts to distribute too
    ///
    /// ### Errors
    /// If drop has already been called for the backstop, the backstop is not the caller,
    /// or the list exceeds the drop amount maximum.
    fn drop(e: Env, list: Map<Address, i128>);
}

#[contractimpl]
impl Emitter for EmitterContract {
    fn initialize(e: Env, blnd_token: Address, backstop: Address, backstop_token: Address) {
        storage::extend_instance(&e);
        if storage::has_blnd_token(&e) {
            panic_with_error!(&e, EmitterError::AlreadyInitialized)
        }

        storage::set_blnd_token(&e, &blnd_token);
        storage::set_backstop(&e, &backstop);
        storage::set_backstop_token(&e, &backstop_token);
        storage::set_last_distro_time(&e, &backstop, e.ledger().timestamp());
    }

    fn distribute(e: Env) -> i128 {
        storage::extend_instance(&e);
        let backstop_address = storage::get_backstop(&e);

        let distribution_amount = emitter::execute_distribute(&e, &backstop_address);

        e.events().publish(
            (Symbol::new(&e, "distribute"),),
            (backstop_address, distribution_amount),
        );
        distribution_amount
    }

    fn get_last_distro(e: Env, backstop_id: Address) -> u64 {
        storage::get_last_distro_time(&e, &backstop_id)
    }

    fn get_backstop(e: Env) -> Address {
        storage::get_backstop(&e)
    }

    fn queue_swap_backstop(e: Env, new_backstop: Address, new_backstop_token: Address) {
        storage::extend_instance(&e);
        let swap =
            backstop_manager::execute_queue_swap_backstop(&e, &new_backstop, &new_backstop_token);

        e.events().publish((Symbol::new(&e, "q_swap"),), swap);
    }

    fn get_queued_swap(e: Env) -> Option<backstop_manager::Swap> {
        storage::get_queued_swap(&e)
    }

    fn cancel_swap_backstop(e: Env) {
        storage::extend_instance(&e);
        let swap = backstop_manager::execute_cancel_swap_backstop(&e);

        e.events().publish((Symbol::new(&e, "del_swap"),), swap);
    }

    fn swap_backstop(e: Env) {
        storage::extend_instance(&e);
        let swap = backstop_manager::execute_swap_backstop(&e);

        e.events().publish((Symbol::new(&e, "swap"),), swap);
    }

    fn drop(e: Env, list: Map<Address, i128>) {
        storage::extend_instance(&e);
        emitter::execute_drop(&e, &list);

        e.events().publish((Symbol::new(&e, "drop"),), list);
    }
}
