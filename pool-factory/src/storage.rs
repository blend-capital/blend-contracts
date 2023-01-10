use soroban_sdk::{contracttype, BytesN, Env};

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(BytesN<32>),
    Wasm,
}

pub trait PoolFactoryStore {
    /// Fetch the factory's WASM hash
    fn get_wasm_hash(&self) -> BytesN<32>;

    /// Set the factory's WASM hash
    ///
    /// ### Arguments
    /// * `wasm_hash` - The WASM hash the factory uses to deploy new contracts
    fn set_wasm_hash(&self, wasm_hash: BytesN<32>);

    /// Check if the factory has a WASM hash set
    fn has_wasm_hash(&self) -> bool;

    /// Check if a given contract_id was deployed by the factory
    ///
    /// ### Arguments
    /// * `contract_id` - The contract_id to check
    fn is_deployed(&self, contract_id: BytesN<32>) -> bool;

    /// Set a contract_id as having been deployed by the factory
    ///
    /// ### Arguments
    /// * `contract_id` - The contract_id that was deployed by the factory
    fn set_deployed(&self, contract_id: BytesN<32>);
}

impl PoolFactoryStore for StorageManager {
    fn get_wasm_hash(&self) -> BytesN<32> {
        self.env()
            .storage()
            .get::<PoolFactoryDataKey, BytesN<32>>(PoolFactoryDataKey::Wasm)
            .unwrap()
            .unwrap()
    }

    fn set_wasm_hash(&self, wasm_hash: BytesN<32>) {
        self.env()
            .storage()
            .set::<PoolFactoryDataKey, BytesN<32>>(PoolFactoryDataKey::Wasm, wasm_hash)
    }

    fn has_wasm_hash(&self) -> bool {
        self.env().storage().has(PoolFactoryDataKey::Wasm)
    }

    fn is_deployed(&self, contract_id: BytesN<32>) -> bool {
        let key = PoolFactoryDataKey::Contracts(contract_id);
        self.env()
            .storage()
            .get::<PoolFactoryDataKey, bool>(key)
            .unwrap_or(Ok(false))
            .unwrap()
    }

    fn set_deployed(&self, contract_id: BytesN<32>) {
        let key = PoolFactoryDataKey::Contracts(contract_id);
        self.env().storage().set::<PoolFactoryDataKey, bool>(key, true);
    }
}

pub struct StorageManager(Env);

impl StorageManager {
    #[inline(always)]
    pub(crate) fn env(&self) -> &Env {
        &self.0
    }

    #[inline(always)]
    pub(crate) fn new(env: &Env) -> StorageManager {
        StorageManager(env.clone())
    }
}
