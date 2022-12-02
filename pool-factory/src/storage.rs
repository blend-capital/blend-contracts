use soroban_sdk::{contracttype, Bytes, BytesN, Env};

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(BytesN<32>),
    Wasm,
}

pub trait PoolFactoryStore {
    /********** Deployed Contracts **********/
    fn get_wasm(&self) -> Bytes;

    fn set_wasm(&self, wasm: Bytes);

    fn has_wasm(&self) -> bool;

    /********** Deployed Contracts **********/

    fn get_deployed(&self, contract: BytesN<32>) -> bool;

    fn set_deployed(&self, contract: BytesN<32>);
}

impl PoolFactoryStore for StorageManager {
    /********** Deployed Contracts **********/
    fn get_wasm(&self) -> Bytes {
        self.env()
            .data()
            .get::<PoolFactoryDataKey, Bytes>(PoolFactoryDataKey::Wasm)
            .unwrap()
            .unwrap()
    }

    fn set_wasm(&self, wasm: Bytes) {
        self.env()
            .data()
            .set::<PoolFactoryDataKey, Bytes>(PoolFactoryDataKey::Wasm, wasm);
    }

    fn has_wasm(&self) -> bool {
        self.env().data().has(PoolFactoryDataKey::Wasm)
    }

    /********** Deployed Contracts **********/

    fn get_deployed(&self, contract: BytesN<32>) -> bool {
        let key = PoolFactoryDataKey::Contracts(contract);
        self.env()
            .data()
            .get::<PoolFactoryDataKey, bool>(key)
            .unwrap_or(Ok(false))
            .unwrap()
    }

    fn set_deployed(&self, contract: BytesN<32>) {
        let key = PoolFactoryDataKey::Contracts(contract);
        self.env().data().set::<PoolFactoryDataKey, bool>(key, true);
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
