use crate::{
    dependencies::TokenClient, emitter, errors::EmitterError, lp_reader::get_lp_blend_holdings,
    storage,
};
use soroban_sdk::{contractimpl, Address, BytesN, Env, Symbol};

/// ### Emitter
///
/// Emits Blend tokens to the backstop module
pub struct EmitterContract;

pub trait EmitterContractTrait {
    /// Initialize the Emitter
    ///
    /// ### Arguments
    /// * `backstop` - The backstop module address
    /// * `blend_id` - The address of the blend token
    /// * `blend_lp_id` - The contract address of the blend LP token contract
    fn initialize(e: Env, backstop: Address, blend_id: BytesN<32>, blend_lp_id: BytesN<32>);

    /// Distributes BLND tokens to the listed backstop module
    ///
    /// Returns the amount of BLND tokens distributed
    ///
    /// ### Errors
    /// If the caller is not the listed backstop module
    fn distribute(e: Env) -> Result<i128, EmitterError>;

    /// Fetch the current backstop
    fn get_bstop(e: Env) -> Address;

    /// Switches the listed backstop module to one with more effective backstop deposits
    ///
    /// Returns OK or an error
    ///
    /// ### Arguments
    /// * `new_backstop` - The contract address of the new backstop module
    ///
    /// ### Errors
    /// If the input contract does not have more backstop deposits than the listed backstop module
    fn swap_bstop(e: Env, new_backstop: Address) -> Result<(), EmitterError>;
}

#[contractimpl]
impl EmitterContractTrait for EmitterContract {
    fn initialize(e: Env, backstop: Address, blend_id: BytesN<32>, blend_lp_id: BytesN<32>) {
        if storage::is_backstop_set(&e) {
            panic!("Emitter already initialized");
        }
        storage::set_backstop(&e, &backstop);
        storage::set_blend_id(&e, &blend_id);
        storage::set_blend_lp_id(&e, &blend_lp_id);
        // TODO: Determine if setting the last distro time here is appropriate, since it means tokens immediately start being distributed
        storage::set_last_distro_time(&e, &e.ledger().timestamp());
    }

    fn distribute(e: Env) -> Result<i128, EmitterError> {
        let backstop = storage::get_backstop(&e);
        // TODO: Authenticate backstop as caller after: https://github.com/stellar/rs-soroban-sdk/issues/868

        let distribution_amount = emitter::execute_distribute(&e, &backstop)?;

        e.events().publish(
            (Symbol::new(&e, "distribute"),),
            (backstop, distribution_amount),
        );
        Ok(distribution_amount)
    }

    fn get_bstop(e: Env) -> Address {
        storage::get_backstop(&e)
    }

    fn swap_bstop(e: Env, new_backstop: Address) -> Result<(), EmitterError> {
        emitter::execute_swap_backstop(&e, new_backstop.clone())?;

        e.events()
            .publish((Symbol::new(&e, "swap"),), (new_backstop,));
        Ok(())
    }
}
