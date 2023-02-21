use soroban_sdk::{contracttype, BytesN, Env};

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(BytesN<32>),
    Wasm,
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
