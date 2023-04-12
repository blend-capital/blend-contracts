use crate::{emitter, errors::EmitterError, storage};
use soroban_sdk::{contractimpl, panic_with_error, Address, BytesN, Env, Symbol};

/// ### Emitter
///
/// Emits Blend tokens to the backstop module
pub struct EmitterContract;

pub trait EmitterContractTrait {
    /// Initialize the Emitter
    ///
    /// ### Arguments
    /// * `backstop_id` - The backstop module BytesN<32> ID
    /// * `blnd_token_id` - The Blend token BytesN<32> ID
    fn initialize(e: Env, backstop: BytesN<32>, blnd_token_id: BytesN<32>);

    /// Distributes BLND tokens to the listed backstop module
    ///
    /// Returns the amount of BLND tokens distributed
    ///
    /// ### Errors
    /// If the caller is not the listed backstop module
    fn distribute(e: Env) -> Result<i128, EmitterError>;

    /// Fetch the current backstop
    fn get_bstop(e: Env) -> BytesN<32>;

    /// Switches the listed backstop module to one with more effective backstop deposits
    ///
    /// Returns OK or an error
    ///
    /// ### Arguments
    /// * `new_backstop_id` - The BytesN<32> ID of the new backstop module
    ///
    /// ### Errors
    /// If the input contract does not have more backstop deposits than the listed backstop module
    fn swap_bstop(e: Env, new_backstop_id: BytesN<32>) -> Result<(), EmitterError>;
}

#[contractimpl]
impl EmitterContractTrait for EmitterContract {
    fn initialize(e: Env, backstop: BytesN<32>, blnd_token_id: BytesN<32>) {
        if storage::has_backstop(&e) {
            panic_with_error!(&e, EmitterError::AlreadyInitialized)
        }

        storage::set_backstop(&e, &backstop);
        storage::set_blend_id(&e, &blnd_token_id);
        // TODO: Determine if setting the last distro time here is appropriate, since it means tokens immediately start being distributed
        storage::set_last_distro_time(&e, &e.ledger().timestamp());
    }

    fn distribute(e: Env) -> Result<i128, EmitterError> {
        let backstop = storage::get_backstop(&e);
        let backstop_addr = Address::from_contract_id(&e, &backstop);
        backstop_addr.require_auth();

        let distribution_amount = emitter::execute_distribute(&e, &backstop_addr)?;

        e.events().publish(
            (Symbol::new(&e, "distribute"),),
            (backstop, distribution_amount),
        );
        Ok(distribution_amount)
    }

    fn get_bstop(e: Env) -> BytesN<32> {
        storage::get_backstop(&e)
    }

    fn swap_bstop(e: Env, new_backstop_id: BytesN<32>) -> Result<(), EmitterError> {
        emitter::execute_swap_backstop(&e, new_backstop_id.clone())?;

        e.events()
            .publish((Symbol::new(&e, "swap"),), (new_backstop_id,));
        Ok(())
    }
}
