use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, Address, Env};

pub(crate) const LEDGER_THRESHOLD_SHARED: u32 = 172800; // ~ 10 days
pub(crate) const LEDGER_BUMP_SHARED: u32 = 241920; // ~ 14 days

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
    // The last block emissions were forked
    LastFork,
}

/// Bump the instance rent for the contract. Bumps for 10 days due to the 7-day cycle window of this contract
pub fn bump_instance(e: &Env) {
    e.storage()
        .instance()
        .bump(LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}

/********** Backstop **********/

/// Fetch the current backstop id
///
/// Returns current backstop module contract address
pub fn get_backstop(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&EmitterDataKey::Backstop)
        .unwrap_optimized()
}

/// Set a new backstop id
///
/// ### Arguments
/// * `new_backstop_id` - The id for the new backstop
pub fn set_backstop(e: &Env, new_backstop_id: &Address) {
    e.storage()
        .instance()
        .set::<EmitterDataKey, Address>(&EmitterDataKey::Backstop, new_backstop_id);
}

/// Check if a backstop has been set
///
/// Returns true if a backstop has been set
pub fn has_backstop(e: &Env) -> bool {
    e.storage().instance().has(&EmitterDataKey::Backstop)
}

/********** Blend **********/

/// Fetch the blend token address
///
/// Returns blend token address
pub fn get_blend_id(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&EmitterDataKey::BlendId)
        .unwrap_optimized()
}

/// Set the blend token address
///
/// ### Arguments
/// * `blend_id` - The blend token address
pub fn set_blend_id(e: &Env, blend_id: &Address) {
    e.storage()
        .instance()
        .set::<EmitterDataKey, Address>(&EmitterDataKey::BlendId, blend_id);
}

/********** Blend Distributions **********/

/// Fetch the last timestamp distribution was ran on
///
/// Returns the last timestamp distribution was ran on
pub fn get_last_distro_time(e: &Env) -> u64 {
    e.storage().persistent().bump(
        &EmitterDataKey::LastDistro,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
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
    e.storage().persistent().bump(
        &EmitterDataKey::LastDistro,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
}

/// Get whether the emitter has performed the drop distribution or not for the current backstop
///
/// Returns true if the emitter has dropped
pub fn get_drop_status(e: &Env) -> bool {
    e.storage().persistent().bump(
        &EmitterDataKey::DropStatus,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
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
    e.storage().persistent().bump(
        &EmitterDataKey::DropStatus,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
}

/// Get the last block an emission fork was executed
///
/// Returns true if the emitter has dropped
pub fn get_last_fork(e: &Env) -> u32 {
    e.storage().persistent().bump(
        &EmitterDataKey::DropStatus,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
    e.storage()
        .persistent()
        .get(&EmitterDataKey::LastFork)
        .unwrap_optimized()
}

/// Set whether the emitter has performed the drop distribution or not for the current backstop
///
/// ### Arguments
/// * `new_status` - new drop status
pub fn set_last_fork(e: &Env, block: u32) {
    e.storage()
        .persistent()
        .set::<EmitterDataKey, u32>(&EmitterDataKey::LastFork, &block);
    e.storage().persistent().bump(
        &EmitterDataKey::LastFork,
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
}
