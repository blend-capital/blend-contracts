use soroban_auth::Identifier;
use soroban_sdk::{contracttype, vec, BytesN, Env, Vec};

/********** Storage Types **********/

/// A deposit that is queued for withdrawal
#[derive(Clone)]
#[contracttype]
pub struct Q4W {
    pub amount: u64, // the amount of shares queued for withdrawal
    pub exp: u64,    // the expiration of the withdrawal
}

/********** Storage Key Types **********/

#[derive(Clone)]
#[contracttype]
pub struct PoolUserKey {
    pool: BytesN<32>,
    user: Identifier,
}

#[derive(Clone)]
#[contracttype]
pub enum BackstopDataKey {
    Shares(PoolUserKey),
    Q4W(PoolUserKey),
    PoolTkn(BytesN<32>),
    PoolShares(BytesN<32>),
    PoolQ4W(BytesN<32>),
}

/********** Storage **********/

pub trait BackstopDataStore {
    /********** Shares **********/

    /// Fetch the balance of shares for a given pool for a user
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    /// * `user` - The owner of the deposit
    fn get_shares(&self, pool: BytesN<32>, user: Identifier) -> u64;

    /// Fetch the total balance of shares for a given pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    fn get_pool_shares(&self, pool: BytesN<32>) -> u64;

    /// Fetch the current withdrawals the user has queued for a given pool
    ///
    /// Returns an empty vec if no q4w's are present
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    /// * `user` - The owner of the deposit
    fn get_q4w(&self, pool: BytesN<32>, user: Identifier) -> Vec<Q4W>;

    /// Fetch the total balance of shares queued for withdraw for a given pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    fn get_pool_q4w(&self, pool: BytesN<32>) -> u64;

    /// Set share balance for a user deposit in a pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    /// * `user` - The owner of the deposit
    /// * `amount` - The amount of shares
    fn set_shares(&self, pool: BytesN<32>, user: Identifier, amount: u64);

    /// Set share deposit total for a pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    /// * `amount` - The amount of shares
    fn set_pool_shares(&self, pool: BytesN<32>, amount: u64);

    /// Set the array of Q4W for a user's deposits in a pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    /// * `user` - The owner of the deposit
    /// * `qw4` - The array of queued withdrawals
    fn set_q4w(&self, pool: BytesN<32>, user: Identifier, q4w: Vec<Q4W>);

    /// Set the total amount of shares queued for withdrawal for a pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop deposit represents
    /// * `amount` - The amount of shares queued for withdrawal for the pool
    fn set_pool_q4w(&self, pool: BytesN<32>, amount: u64);

    /********** Tokens **********/

    /// Get the balance of tokens in the backstop for a pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop balance belongs to
    fn get_pool_tokens(&self, pool: BytesN<32>) -> u64;

    /// Set the balance of tokens in the backstop for a pool
    ///
    /// ### Arguments
    /// * `pool` - The pool the backstop balance belongs to
    /// * `amount` - The amount of tokens attributed to the pool
    fn set_pool_tokens(&self, pool: BytesN<32>, amount: u64);
}

pub struct StorageManager(Env);

impl BackstopDataStore for StorageManager {
    /********** Shares **********/

    fn get_shares(&self, pool: BytesN<32>, user: Identifier) -> u64 {
        let key = BackstopDataKey::Shares(PoolUserKey { pool, user });
        self.env()
            .data()
            .get::<BackstopDataKey, u64>(key)
            .unwrap_or(Ok(0))
            .unwrap()
    }

    fn get_pool_shares(&self, pool: BytesN<32>) -> u64 {
        let key = BackstopDataKey::PoolShares(pool);
        self.env()
            .data()
            .get::<BackstopDataKey, u64>(key)
            .unwrap_or(Ok(0))
            .unwrap()
    }

    fn get_q4w(&self, pool: BytesN<32>, user: Identifier) -> Vec<Q4W> {
        let key = BackstopDataKey::Q4W(PoolUserKey { pool, user });
        self.env()
            .data()
            .get::<BackstopDataKey, Vec<Q4W>>(key)
            .unwrap_or(Ok(vec![&self.env()]))
            .unwrap()
    }

    fn get_pool_q4w(&self, pool: BytesN<32>) -> u64 {
        let key = BackstopDataKey::PoolQ4W(pool);
        self.env()
            .data()
            .get::<BackstopDataKey, u64>(key)
            .unwrap_or(Ok(0))
            .unwrap()
    }

    fn set_shares(&self, pool: BytesN<32>, user: Identifier, amount: u64) {
        let key = BackstopDataKey::Shares(PoolUserKey { pool, user });
        self.env().data().set::<BackstopDataKey, u64>(key, amount);
    }

    fn set_pool_shares(&self, pool: BytesN<32>, amount: u64) {
        let key = BackstopDataKey::PoolShares(pool);
        self.env().data().set::<BackstopDataKey, u64>(key, amount);
    }

    fn set_q4w(&self, pool: BytesN<32>, user: Identifier, q4w: Vec<Q4W>) {
        let key = BackstopDataKey::Q4W(PoolUserKey { pool, user });
        self.env().data().set::<BackstopDataKey, Vec<Q4W>>(key, q4w);
    }

    fn set_pool_q4w(&self, pool: BytesN<32>, amount: u64) {
        let key = BackstopDataKey::PoolQ4W(pool);
        self.env().data().set::<BackstopDataKey, u64>(key, amount);
    }

    /********** Tokens **********/

    fn get_pool_tokens(&self, pool: BytesN<32>) -> u64 {
        let key = BackstopDataKey::PoolTkn(pool);
        self.env()
            .data()
            .get::<BackstopDataKey, u64>(key)
            .unwrap_or(Ok(0))
            .unwrap()
    }

    fn set_pool_tokens(&self, pool: BytesN<32>, amount: u64) {
        let key = BackstopDataKey::PoolTkn(pool);
        self.env().data().set::<BackstopDataKey, u64>(key, amount);
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
