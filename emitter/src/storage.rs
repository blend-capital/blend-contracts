use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, Address, Env};

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
}

/********** Backstop **********/

/// Fetch the current backstop id
///
/// Returns current backstop module contract address
pub fn get_backstop(e: &Env) -> Address {
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
        .set::<EmitterDataKey, Address>(&EmitterDataKey::Backstop, &new_backstop_id);
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
        .set::<EmitterDataKey, Address>(&EmitterDataKey::BlendId, &blend_id);
}

/********** Blend Distributions **********/

/// Fetch the last timestamp distribution was ran on
///
/// Returns the last timestamp distribution was ran on
pub fn get_last_distro_time(e: &Env) -> u64 {
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
