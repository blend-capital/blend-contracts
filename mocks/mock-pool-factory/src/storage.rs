use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, Address, BytesN, Env};

/********** Ledger Thresholds **********/

const ONE_DAY_LEDGERS: u32 = 17280; // assumes 5s a ledger

const LEDGER_THRESHOLD_INSTANCE: u32 = ONE_DAY_LEDGERS * 30; // ~ 30 days
const LEDGER_BUMP_INSTANCE: u32 = LEDGER_THRESHOLD_INSTANCE + ONE_DAY_LEDGERS; // ~ 31 days

const LEDGER_THRESHOLD_SHARED: u32 = ONE_DAY_LEDGERS * 45; // ~ 45 days
const LEDGER_BUMP_SHARED: u32 = LEDGER_THRESHOLD_SHARED + ONE_DAY_LEDGERS; // ~ 46 days

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(Address),
    PoolInitMeta,
}

#[derive(Clone)]
#[contracttype]
pub struct PoolInitMeta {
    pub pool_hash: BytesN<32>,
    pub backstop: Address,
    pub blnd_id: Address,
}

/// Bump the instance rent for the contract
pub fn extend_instance(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(LEDGER_THRESHOLD_INSTANCE, LEDGER_BUMP_INSTANCE);
}

/// Fetch the pool initialization metadata
pub fn get_pool_init_meta(e: &Env) -> PoolInitMeta {
    e.storage()
        .instance()
        .get::<PoolFactoryDataKey, PoolInitMeta>(&PoolFactoryDataKey::PoolInitMeta)
        .unwrap_optimized()
}

/// Set the pool initialization metadata
///
/// ### Arguments
/// * `pool_init_meta` - The metadata to initialize pools
pub fn set_pool_init_meta(e: &Env, pool_init_meta: &PoolInitMeta) {
    e.storage()
        .instance()
        .set::<PoolFactoryDataKey, PoolInitMeta>(&PoolFactoryDataKey::PoolInitMeta, pool_init_meta)
}

/// Check if the factory has a WASM hash set
pub fn has_pool_init_meta(e: &Env) -> bool {
    e.storage()
        .instance()
        .has(&PoolFactoryDataKey::PoolInitMeta)
}

/// Check if a given contract_id was deployed by the factory
///
/// ### Arguments
/// * `contract_id` - The contract_id to check
pub fn is_deployed(e: &Env, contract_id: &Address) -> bool {
    let key = PoolFactoryDataKey::Contracts(contract_id.clone());
    if let Some(result) = e
        .storage()
        .persistent()
        .get::<PoolFactoryDataKey, bool>(&key)
    {
        e.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
        result
    } else {
        false
    }
}

/// Set a contract_id as having been deployed by the factory
///
/// ### Arguments
/// * `contract_id` - The contract_id that was deployed by the factory
pub fn set_deployed(e: &Env, contract_id: &Address) {
    let key = PoolFactoryDataKey::Contracts(contract_id.clone());
    e.storage()
        .persistent()
        .set::<PoolFactoryDataKey, bool>(&key, &true);
    e.storage()
        .persistent()
        .extend_ttl(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
}
