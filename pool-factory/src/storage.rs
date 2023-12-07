use soroban_sdk::{contracttype, unwrap::UnwrapOptimized, Address, BytesN, Env, Symbol};

// @dev: This contract is not expected to be used often, so we can use a higher bump amount
pub(crate) const LEDGER_THRESHOLD: u32 = 518400; // TODO: Check on phase 1 max ledger entry bump
pub(crate) const LEDGER_BUMP: u32 = 535670; // TODO: Check on phase 1 max ledger entry bump

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(Address),
}

#[derive(Clone)]
#[contracttype]
pub struct PoolInitMeta {
    pub pool_hash: BytesN<32>,
    pub backstop: Address,
    pub blnd_id: Address,
    pub usdc_id: Address, //Must have 7 token decimals due to lot decimal restriction in backstop interest auctions
}

/// Bump the instance rent for the contract
pub fn extend_instance(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(LEDGER_THRESHOLD, LEDGER_BUMP);
}

/// Fetch the pool initialization metadata
pub fn get_pool_init_meta(e: &Env) -> PoolInitMeta {
    e.storage()
        .instance()
        .get::<Symbol, PoolInitMeta>(&Symbol::new(e, "PoolMeta"))
        .unwrap_optimized()
}

/// Set the pool initialization metadata
///
/// ### Arguments
/// * `pool_init_meta` - The metadata to initialize pools
pub fn set_pool_init_meta(e: &Env, pool_init_meta: &PoolInitMeta) {
    e.storage()
        .instance()
        .set::<Symbol, PoolInitMeta>(&Symbol::new(e, "PoolMeta"), pool_init_meta)
}

/// Check if the factory has a WASM hash set
pub fn has_pool_init_meta(e: &Env) -> bool {
    e.storage().instance().has(&Symbol::new(e, "PoolMeta"))
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
            .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
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
        .extend_ttl(&key, LEDGER_THRESHOLD, LEDGER_BUMP);
}
