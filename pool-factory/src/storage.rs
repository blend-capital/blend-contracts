use soroban_sdk::{contracttype, BytesN, Env};

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(BytesN<32>),
    Wasm,
}

/// Fetch the factory's WASM hash
pub fn get_wasm_hash(e: &Env) -> BytesN<32> {
    e.storage()
        .get::<PoolFactoryDataKey, BytesN<32>>(&PoolFactoryDataKey::Wasm)
        .unwrap()
        .unwrap()
}

/// Set the factory's WASM hash
///
/// ### Arguments
/// * `wasm_hash` - The WASM hash the factory uses to deploy new contracts
pub fn set_wasm_hash(e: &Env, wasm_hash: &BytesN<32>) {
    e.storage()
        .set::<PoolFactoryDataKey, BytesN<32>>(&PoolFactoryDataKey::Wasm, wasm_hash)
}

/// Check if the factory has a WASM hash set
pub fn has_wasm_hash(e: &Env) -> bool {
    e.storage().has(&PoolFactoryDataKey::Wasm)
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
