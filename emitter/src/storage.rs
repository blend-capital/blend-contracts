use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, Address, Env, Symbol};

use crate::backstop_manager::Swap;

/********** Ledger Thresholds **********/

const ONE_DAY_LEDGERS: u32 = 17280; // assumes 5s a ledger

const LEDGER_THRESHOLD_INSTANCE: u32 = ONE_DAY_LEDGERS * 30; // ~ 30 days
const LEDGER_BUMP_INSTANCE: u32 = LEDGER_THRESHOLD_INSTANCE + ONE_DAY_LEDGERS; // ~ 31 days

const LEDGER_THRESHOLD_SHARED: u32 = ONE_DAY_LEDGERS * 45; // ~ 45 days
const LEDGER_BUMP_SHARED: u32 = LEDGER_THRESHOLD_SHARED + ONE_DAY_LEDGERS; // ~ 46 days

/********** Storage **********/

const IS_INIT_KEY: &str = "IsInit";
const BACKSTOP_KEY: &str = "Backstop";
const BACKSTOP_TOKEN_KEY: &str = "BToken";
const BLND_TOKEN_KEY: &str = "BLNDTkn";
const SWAP_KEY: &str = "Swap";

// Emitter Data Keys
#[derive(Clone)]
#[contracttype]
pub enum EmitterDataKey {
    // The last timestamp distribution was ran on
    LastDistro(Address),
    // Stores the list of backstop addresses that have dropped
    Dropped(Address),
}

/// Bump the instance rent for the contract
pub fn extend_instance(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(LEDGER_THRESHOLD_INSTANCE, LEDGER_BUMP_INSTANCE);
}

/********** Init **********/

/// Check if the contract has been initialized
pub fn get_is_init(e: &Env) -> bool {
    e.storage().instance().has(&Symbol::new(e, IS_INIT_KEY))
}

/// Set the contract as initialized
pub fn set_is_init(e: &Env) {
    e.storage()
        .instance()
        .set::<Symbol, bool>(&Symbol::new(e, IS_INIT_KEY), &true);
}

/********** Backstop **********/

/// Fetch the current backstop address
///
/// Returns current backstop module contract address
pub fn get_backstop(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&Symbol::new(e, BACKSTOP_KEY))
        .unwrap_optimized()
}

/// Set a new backstop address
///
/// ### Arguments
/// * `new_backstop` - The new backstop module contract address
pub fn set_backstop(e: &Env, new_backstop: &Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, BACKSTOP_KEY), new_backstop);
}

/// Fetch the current backstop token address
///
/// Returns current backstop module contract address
pub fn get_backstop_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&Symbol::new(e, BACKSTOP_TOKEN_KEY))
        .unwrap_optimized()
}

/// Set a new backstop token address
///
/// ### Arguments
/// * `new_backstop_token` - The new backstop token contract address
pub fn set_backstop_token(e: &Env, new_backstop_token: &Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, BACKSTOP_TOKEN_KEY), new_backstop_token);
}

/// Fetch the current queued backstop swap, or None
pub fn get_queued_swap(e: &Env) -> Option<Swap> {
    if let Some(result) = e.storage().persistent().get(&Symbol::new(e, SWAP_KEY)) {
        e.storage().persistent().extend_ttl(
            &Symbol::new(e, SWAP_KEY),
            LEDGER_THRESHOLD_SHARED,
            LEDGER_BUMP_SHARED,
        );
        Some(result)
    } else {
        None
    }
}

/// Set a new swap in the queue
///
/// ### Arguments
/// * `swap` - The swap to queue
pub fn set_queued_swap(e: &Env, swap: &Swap) {
    e.storage()
        .persistent()
        .set::<Symbol, Swap>(&Symbol::new(e, SWAP_KEY), swap);
    e.storage().persistent().extend_ttl(
        &Symbol::new(e, SWAP_KEY),
        LEDGER_THRESHOLD_SHARED,
        LEDGER_BUMP_SHARED,
    );
}

/// Delete the current queued backstop swap
pub fn del_queued_swap(e: &Env) {
    e.storage().persistent().remove(&Symbol::new(e, SWAP_KEY));
}

/********** Blend **********/

/// Fetch the BLND token address
///
/// Returns blend token address
pub fn get_blnd_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&Symbol::new(e, BLND_TOKEN_KEY))
        .unwrap_optimized()
}

/// Set the BLND token address
///
/// ### Arguments
/// * `BLND` - The blend token address
pub fn set_blnd_token(e: &Env, blnd_token: &Address) {
    e.storage()
        .instance()
        .set::<Symbol, Address>(&Symbol::new(e, BLND_TOKEN_KEY), blnd_token);
}

/********** Blend Distributions **********/

/// Fetch the last timestamp distribution was ran on
///
/// Returns the last timestamp distribution was ran on
///
/// ### Arguments
/// * `backstop` - The backstop module Address
pub fn get_last_distro_time(e: &Env, backstop: &Address) -> u64 {
    // don't need to bump while reading since this value is set on every distribution
    e.storage()
        .persistent()
        .get(&EmitterDataKey::LastDistro(backstop.clone()))
        .unwrap_optimized()
}

/// Set the last timestamp distribution was ran on
///
/// ### Arguments
/// * `backstop` - The backstop module Address
/// * `last_distro` - The last timestamp distribution was ran on
pub fn set_last_distro_time(e: &Env, backstop: &Address, last_distro: u64) {
    let key = EmitterDataKey::LastDistro(backstop.clone());
    e.storage()
        .persistent()
        .set::<EmitterDataKey, u64>(&key, &last_distro);
    e.storage()
        .persistent()
        .extend_ttl(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}

/// Get whether the emitter has performed the drop distribution or not for the current backstop
///
/// Returns true if the emitter has dropped
pub fn get_drop_status(e: &Env, backstop: &Address) -> bool {
    e.storage()
        .persistent()
        .get::<EmitterDataKey, bool>(&EmitterDataKey::Dropped(backstop.clone()))
        .unwrap_or(false)
}

/// Set whether the emitter has performed the drop distribution or not for the current backstop
///
/// ### Arguments
/// * `new_status` - new drop status
pub fn set_drop_status(e: &Env, backstop: &Address) {
    e.storage()
        .persistent()
        .set::<EmitterDataKey, bool>(&EmitterDataKey::Dropped(backstop.clone()), &true);
}
