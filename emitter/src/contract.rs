use crate::{dependencies::BackstopClient, emitter, errors::EmitterError, storage};
use soroban_sdk::{contractimpl, symbol, Address, BytesN, Env};

/// ### Emitter
///
/// Emits Blend tokens to the backstop module
pub struct EmitterContract;

pub trait EmitterContractTrait {
    /// Initialize the Emitter
    ///
    /// ### Arguments
    /// * `backstop` - The backstop module address
    /// * `backstop_id` - The backstop module contract address
    /// * `blend_token_id` - The address of the blend token
    fn initialize(e: Env, backstop: Address, backstop_id: BytesN<32>, blend_token_id: BytesN<32>);

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
    fn swap_bstop(
        e: Env,
        new_backstop: Address,
        new_backstop_id: BytesN<32>,
    ) -> Result<(), EmitterError>;
}

#[contractimpl]
impl EmitterContractTrait for EmitterContract {
    fn initialize(e: Env, backstop: Address, backstop_id: BytesN<32>, blend_token_id: BytesN<32>) {
        if storage::has_backstop(&e) {
            panic!("Emitter already initialized");
        }
        storage::set_backstop(&e, &backstop);
        storage::set_blend_id(&e, &blend_token_id);
        let backstop_token = BackstopClient::new(&e, &backstop_id).bstp_token();
        storage::set_backstop_token_id(&e, &backstop_token);
        // TODO: Determine if setting the last distro time here is appropriate, since it means tokens immediately start being distributed
        storage::set_last_distro_time(&e, &e.ledger().timestamp());
    }

    fn distribute(e: Env) -> Result<i128, EmitterError> {
        let backstop = storage::get_backstop(&e);
        // TODO: Authenticate backstop as caller after: https://github.com/stellar/rs-soroban-sdk/issues/868

        let distribution_amount = emitter::execute_distribute(&e, &backstop)?;

        e.events()
            .publish((symbol!("distribute"),), (backstop, distribution_amount));
        Ok(distribution_amount)
    }

    fn get_bstop(e: Env) -> Address {
        storage::get_backstop(&e)
    }

    fn swap_bstop(
        e: Env,
        new_backstop: Address,
        new_backstop_id: BytesN<32>,
    ) -> Result<(), EmitterError> {
        emitter::execute_swap_backstop(&e, new_backstop.clone())?;
        let backstop_token = BackstopClient::new(&e, &new_backstop_id).bstp_token();
        storage::set_backstop_token_id(&e, &backstop_token);

        e.events().publish((symbol!("swap"),), (new_backstop,));
        Ok(())
    }
}
