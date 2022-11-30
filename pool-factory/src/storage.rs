use soroban_sdk::{contracttype, BytesN, Env};

#[derive(Clone)]
#[contracttype]
pub enum PoolFactoryDataKey {
    Contracts(BytesN<32>),
}

pub trait PoolFactoryStore {
    /********** Deployed Contracts **********/

    fn get_deployed(self, contract: BytesN<32>) -> bool;

    fn set_deployed(self, contract: BytesN<32>);
}

impl PoolFactoryStore for StorageManager {
    /********** Deployed Contracts **********/

    fn get_deployed(self, contract: BytesN<32>) -> bool {
        let key = PoolFactoryDataKey::Contracts(contract);
        self.env()
            .data()
            .get::<PoolFactoryDataKey, bool>(key)
            .unwrap_or(Ok(false))
            .unwrap()
    }

    fn set_deployed(self, contract: BytesN<32>) {
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
