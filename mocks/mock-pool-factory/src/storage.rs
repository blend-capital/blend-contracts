use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, Address, BytesN, Env};

pub(crate) const INSTANCE_BUMP_AMOUNT: u32 = 34560; // 2 days
pub(crate) const CYCLE_BUMP_AMOUNT: u32 = 69120; // 10 days - use for shared data accessed on the 7-day cycle window
pub(crate) const USER_BUMP_AMOUNT: u32 = 518400; // 30 days

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
    pub usdc_id: Address,
}

/// Bump the instance rent for the contract
pub fn bump_instance(e: &Env) {
    e.storage().instance().bump(INSTANCE_BUMP_AMOUNT);
}

/// Fetch the pool initialization metadata
pub fn get_pool_init_meta(e: &Env) -> PoolInitMeta {
    // TODO: Change to instance - https://github.com/stellar/rs-soroban-sdk/issues/1040
    e.storage()
        .persistent()
        .bump(&PoolFactoryDataKey::PoolInitMeta, USER_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolFactoryDataKey, PoolInitMeta>(&PoolFactoryDataKey::PoolInitMeta)
        .unwrap_optimized()
}

/// Set the pool initialization metadata
///
/// ### Arguments
/// * `pool_init_meta` - The metadata to initialize pools
pub fn set_pool_init_meta(e: &Env, pool_init_meta: &PoolInitMeta) {
    e.storage()
        .persistent()
        .set::<PoolFactoryDataKey, PoolInitMeta>(&PoolFactoryDataKey::PoolInitMeta, pool_init_meta)
}

/// Check if the factory has a WASM hash set
pub fn has_pool_init_meta(e: &Env) -> bool {
    e.storage()
        .persistent()
        .has(&PoolFactoryDataKey::PoolInitMeta)
}

/// Check if a given contract_id was deployed by the factory
///
/// ### Arguments
/// * `contract_id` - The contract_id to check
pub fn is_deployed(e: &Env, contract_id: &Address) -> bool {
    let key = PoolFactoryDataKey::Contracts(contract_id.clone());
    e.storage().persistent().bump(&key, CYCLE_BUMP_AMOUNT);
    e.storage()
        .persistent()
        .get::<PoolFactoryDataKey, bool>(&key)
        .unwrap_or(false)
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
}
