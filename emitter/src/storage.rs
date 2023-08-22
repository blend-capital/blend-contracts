use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, Address, Env};

pub(crate) const SHARED_BUMP_AMOUNT: u32 = 69120; // 4 days
pub(crate) const CYCLE_BUMP_AMOUNT: u32 = 69120; // 10 days - use for shared data accessed on the 7-day cycle window

/********** Storage **********/

// Emitter Data Keys
#[derive(Clone)]
#[contracttype]
pub enum EmitterDataKey {
    // The address of the backstop module contract
    Backstop,
    /// TODO: Delete after address <-> bytesN support,
    BstopId,
    // The address of the blend token contract
    BlendId,
    // The address of the blend lp token contract
    BlendLPId,
    // The last timestamp distribution was ran on
    LastDistro,
    // The drop status for the current backstop
    DropStatus,
}

/// Bump the instance rent for the contract. Bumps for 10 days due to the 7-day cycle window of this contract
pub fn bump_instance(e: &Env) {
    e.storage().instance().bump(CYCLE_BUMP_AMOUNT);
}

/********** Backstop **********/

/// Fetch the current backstop id
///
/// Returns current backstop module contract address
pub fn get_backstop(e: &Env) -> Address {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&EmitterDataKey::Backstop, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&EmitterDataKey::Backstop)
        .unwrap_optimized()
}

/// Set a new backstop id
///
/// ### Arguments
/// * `new_backstop_id` - The id for the new backstop
pub fn set_backstop(e: &Env, new_backstop_id: &Address) {
    e.storage()
        .persistent()
        .set::<EmitterDataKey, Address>(&EmitterDataKey::Backstop, new_backstop_id);
}

/// Check if a backstop has been set
///
/// Returns true if a backstop has been set
pub fn has_backstop(e: &Env) -> bool {
    e.storage().persistent().has(&EmitterDataKey::Backstop)
}

/********** Blend **********/

/// Fetch the blend token address
///
/// Returns blend token address
pub fn get_blend_id(e: &Env) -> Address {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&EmitterDataKey::BlendId, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&EmitterDataKey::BlendId)
        .unwrap_optimized()
}

/// Set the blend token address
///
/// ### Arguments
/// * `blend_id` - The blend token address
pub fn set_blend_id(e: &Env, blend_id: &Address) {
    e.storage()
        .persistent()
        .set::<EmitterDataKey, Address>(&EmitterDataKey::BlendId, blend_id);
}

/********** Blend Distributions **********/

/// Fetch the last timestamp distribution was ran on
///
/// Returns the last timestamp distribution was ran on
pub fn get_last_distro_time(e: &Env) -> u64 {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&EmitterDataKey::LastDistro, CYCLE_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&EmitterDataKey::LastDistro)
        .unwrap_optimized()
}

/// Set the last timestamp distribution was ran on
///
/// ### Arguments
/// * `last_distro` - The last timestamp distribution was ran on
pub fn set_last_distro_time(e: &Env, last_distro: &u64) {
    e.storage()
        .persistent()
        .set::<EmitterDataKey, u64>(&EmitterDataKey::LastDistro, last_distro);
}

/// Get whether the emitter has performed the drop distribution or not for the current backstop
///
/// Returns true if the emitter has dropped
pub fn get_drop_status(e: &Env) -> bool {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&EmitterDataKey::DropStatus, SHARED_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get(&EmitterDataKey::DropStatus)
        .unwrap_optimized()
}

/// Set whether the emitter has performed the drop distribution or not for the current backstop
///
/// ### Arguments
/// * `new_status` - new drop status
pub fn set_drop_status(e: &Env, new_status: bool) {
    e.storage()
        .persistent()
        .set::<EmitterDataKey, bool>(&EmitterDataKey::DropStatus, &new_status);
}
