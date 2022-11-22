use soroban_auth::Identifier;
use soroban_sdk::{contracttype, BytesN, Env};

/********** Storage **********/

// Emitter Data Keys
#[derive(Clone)]
#[contracttype]
pub enum EmitterDataKey {
    // The address of the backstop module contract
    Backstop,
    // The address of the blend token contract
    BlendId,
    // The address of the blend lp token contract
    BlendLPId,
    // The last timestamp distribution was ran on
    LastDistro,
}

pub trait EmitterDataStore {
    /********** Backstop **********/

    /// Fetch the current backstop Identifier
    ///
    /// Returns current backstop module contract address
    ///
    fn get_backstop_id(&self) -> Identifier;

    /// Set a new backstop
    ///
    /// ### Arguments
    /// * `new_backstop` - The Identifier for the new backstop
    fn set_backstop_id(&self, new_backstop: Identifier);

    /********** Blend **********/

    /// Fetch the blend token address
    ///
    /// Returns blend token address
    ///
    fn get_blend_id(&self) -> BytesN<32>;

    /// Set the blend token address
    ///
    /// ### Arguments
    /// * `blend_id` - The blend token address
    fn set_blend_id(&self, blend_id: BytesN<32>);

    /// Fetch the lp token address
    ///
    /// Returns the blend lp token address
    ///
    fn get_blend_lp_id(&self) -> Identifier;

    /// Set the lp token address
    ///
    /// ### Arguments
    /// * `blend_lp_id` - The blend lp token address
    fn set_blend_lp_id(&self, blend_lp_id: Identifier);

    /********** Blend Distributions **********/

    /// Fetch the last timestamp distribution was ran on
    ///
    /// Returns the last timestamp distribution was ran on
    ///
    fn get_last_distro_time(&self) -> u64;

    /// Set the last timestamp distribution was ran on
    ///
    /// ### Arguments
    /// * `last_distro` - The last timestamp distribution was ran on
    fn set_last_distro_time(&self, last_distro: u64);
}

pub struct StorageManager(Env);

impl EmitterDataStore for StorageManager {
    /********** Backstop **********/

    fn get_backstop_id(&self) -> Identifier {
        self.env()
            .data()
            .get_unchecked(EmitterDataKey::Backstop)
            .unwrap()
    }

    fn set_backstop_id(&self, new_backstop: Identifier) {
        self.env()
            .data()
            .set::<EmitterDataKey, Identifier>(EmitterDataKey::Backstop, new_backstop);
    }

    /********** Blend **********/

    fn get_blend_id(&self) -> BytesN<32> {
        self.env()
            .data()
            .get_unchecked(EmitterDataKey::BlendId)
            .unwrap()
    }

    fn set_blend_id(&self, blend_id: BytesN<32>) {
        self.env()
            .data()
            .set::<EmitterDataKey, BytesN<32>>(EmitterDataKey::BlendId, blend_id);
    }

    fn get_blend_lp_id(&self) -> Identifier {
        self.env()
            .data()
            .get_unchecked(EmitterDataKey::BlendLPId)
            .unwrap()
    }

    fn set_blend_lp_id(&self, blend_lp_id: Identifier) {
        self.env()
            .data()
            .set::<EmitterDataKey, Identifier>(EmitterDataKey::BlendLPId, blend_lp_id);
    }

    /********** Blend Distributions **********/

    fn get_last_distro_time(&self) -> u64 {
        self.env()
            .data()
            .get_unchecked(EmitterDataKey::LastDistro)
            .unwrap()
    }

    fn set_last_distro_time(&self, last_distro: u64) {
        self.env()
            .data()
            .set::<EmitterDataKey, u64>(EmitterDataKey::LastDistro, last_distro);
    }
}

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
