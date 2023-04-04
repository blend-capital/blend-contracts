use soroban_sdk::{contracttype, Address, BytesN, Env};

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

/// Fetch the current backstop
///
/// Returns current backstop module contract address
pub fn get_backstop(e: &Env) -> Address {
    e.storage()
        .get_unchecked(&EmitterDataKey::Backstop)
        .unwrap()
}

/// Set a new backstop
///
/// ### Arguments
/// * `new_backstop` - The id for the new backstop
pub fn set_backstop(e: &Env, new_backstop: &Address) {
    e.storage()
        .set::<EmitterDataKey, Address>(&EmitterDataKey::Backstop, &new_backstop);
}

/// Check if a backstop has been set
///
/// Returns true if a backstop has been set
pub fn has_backstop(e: &Env) -> bool {
    e.storage().has(&EmitterDataKey::Backstop)
}

/********** Blend **********/

/// Fetch the blend token address
///
/// Returns blend token address
pub fn get_blend_id(e: &Env) -> BytesN<32> {
    e.storage().get_unchecked(&EmitterDataKey::BlendId).unwrap()
}

/// Set the blend token address
///
/// ### Arguments
/// * `blend_id` - The blend token address
pub fn set_blend_id(e: &Env, blend_id: &BytesN<32>) {
    e.storage()
        .set::<EmitterDataKey, BytesN<32>>(&EmitterDataKey::BlendId, &blend_id);
}

/// Fetch the backstop token address
///
/// Returns the blend lp token address
pub fn get_backstop_token_id(e: &Env) -> BytesN<32> {
    e.storage()
        .get_unchecked(&EmitterDataKey::BlendLPId)
        .unwrap()
}

/// Set the lp token address
///
/// ### Arguments
/// * `blend_lp_id` - The blend lp token address
pub fn set_backstop_token_id(e: &Env, blend_lp_id: &BytesN<32>) {
    e.storage()
        .set::<EmitterDataKey, BytesN<32>>(&EmitterDataKey::BlendLPId, blend_lp_id);
}

/********** Blend Distributions **********/

/// Fetch the last timestamp distribution was ran on
///
/// Returns the last timestamp distribution was ran on
pub fn get_last_distro_time(e: &Env) -> u64 {
    e.storage()
        .get_unchecked(&EmitterDataKey::LastDistro)
        .unwrap()
}

/// Set the last timestamp distribution was ran on
///
/// ### Arguments
/// * `last_distro` - The last timestamp distribution was ran on
pub fn set_last_distro_time(e: &Env, last_distro: &u64) {
    e.storage()
        .set::<EmitterDataKey, u64>(&EmitterDataKey::LastDistro, last_distro);
}
