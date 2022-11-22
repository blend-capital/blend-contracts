use crate::{
    dependencies::TokenClient,
    errors::EmitterError,
    storage::{EmitterDataKey, EmitterDataStore, StorageManager},
};
use soroban_auth::Identifier;
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
    fn swap_bstop(e: Env, asset: BytesN<32>, amount: BigInt) -> Result<(), EmitterError>;
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
    }

    fn distribute(e: Env) -> Result<BigInt, EmitterError> {}

    fn swap_bstop(e: Env, asset: BytesN<32>, amount: BigInt) -> Result<(), EmitterError> {}
}

// ****** Helpers *****

fn get_contract_id(e: &Env) -> Identifier {
    Identifier::Contract(e.current_contract())
}
