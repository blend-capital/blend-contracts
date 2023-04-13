use soroban_sdk::{contracttype, BytesN, Env};

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(BytesN<32>),
    PoolInitMeta,
}

#[derive(Clone)]
#[contracttype]
pub struct PoolInitMeta {
    pub pool_hash: BytesN<32>,
    pub b_token_hash: BytesN<32>,
    pub d_token_hash: BytesN<32>,
    pub backstop: BytesN<32>,
    pub blnd_id: BytesN<32>,
    pub usdc_id: BytesN<32>,
}

/// Fetch the pool initialization metadata
pub fn get_pool_init_meta(e: &Env) -> PoolInitMeta {
    e.storage()
        .get::<PoolFactoryDataKey, PoolInitMeta>(&PoolFactoryDataKey::PoolInitMeta)
        .unwrap()
        .unwrap()
}

/// Set the pool initialization metadata
///
/// ### Arguments
/// * `pool_init_meta` - The metadata to initialize pools
pub fn set_pool_init_meta(e: &Env, pool_init_meta: &PoolInitMeta) {
    e.storage()
        .set::<PoolFactoryDataKey, PoolInitMeta>(&PoolFactoryDataKey::PoolInitMeta, pool_init_meta)
}

/// Check if the factory has a WASM hash set
pub fn has_pool_init_meta(e: &Env) -> bool {
    e.storage().has(&PoolFactoryDataKey::PoolInitMeta)
}

/// Check if a given contract_id was deployed by the factory
///
/// ### Arguments
/// * `contract_id` - The contract_id to check
pub fn is_deployed(e: &Env, contract_id: &BytesN<32>) -> bool {
    let key = PoolFactoryDataKey::Contracts(contract_id.clone());
    e.storage()
        .get::<PoolFactoryDataKey, bool>(&key)
        .unwrap_or(Ok(false))
        .unwrap()
}
/// Set a contract_id as having been deployed by the factory
///
/// ### Arguments
/// * `contract_id` - The contract_id that was deployed by the factory
pub fn set_deployed(e: &Env, contract_id: &BytesN<32>) {
    let key = PoolFactoryDataKey::Contracts(contract_id.clone());
    e.storage().set::<PoolFactoryDataKey, bool>(&key, &true);
}
