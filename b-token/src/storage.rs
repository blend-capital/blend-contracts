use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env};

/********** Storage Types **********/

#[derive(Clone)]
#[contracttype]
pub struct Asset {
    pub id: BytesN<32>,
    pub res_index: u32,
}

/********** Storage Key Types **********/

#[derive(Clone)]
#[contracttype]
pub struct AllowanceDataKey {
    pub from: Address,
    pub spender: Address,
}

#[derive(Clone)]
#[contracttype]
pub enum TokenDataKey {
    Allowance(AllowanceDataKey),
    Balance(Address),
    Pool,
    PoolId,
    Asset,
    Decimals,
    Name,
    Symbol,
}

/********** Storage Helpers **********/

/***** Allowance *****/

pub fn read_allowance(e: &Env, from: &Address, spender: &Address) -> i128 {
    let key = TokenDataKey::Allowance(AllowanceDataKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    e.storage()
        .get::<TokenDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

pub fn write_allowance(e: &Env, from: &Address, spender: &Address, amount: &i128) {
    let key = TokenDataKey::Allowance(AllowanceDataKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    e.storage().set::<TokenDataKey, i128>(&key, amount)
}

/***** Balance *****/

pub fn read_balance(e: &Env, user: &Address) -> i128 {
    let key = TokenDataKey::Balance(user.clone());
    // addresses are authorized by default
    e.storage()
        .get::<TokenDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

pub fn write_balance(e: &Env, user: &Address, balance: &i128) {
    let key = TokenDataKey::Balance(user.clone());
    e.storage().set::<TokenDataKey, i128>(&key, balance)
}

/***** Pool *****/

pub fn read_pool(e: &Env) -> Address {
    e.storage()
        .get_unchecked::<TokenDataKey, Address>(&TokenDataKey::Pool)
        .unwrap()
}

pub fn has_pool(e: &Env) -> bool {
    e.storage().has::<TokenDataKey>(&TokenDataKey::Pool)
}

pub fn write_pool(e: &Env, pool: &Address) {
    e.storage()
        .set::<TokenDataKey, Address>(&TokenDataKey::Pool, pool)
}

pub fn read_pool_id(e: &Env) -> BytesN<32> {
    e.storage()
        .get_unchecked::<TokenDataKey, BytesN<32>>(&TokenDataKey::PoolId)
        .unwrap()
}

pub fn write_pool_id(e: &Env, pool_id: &BytesN<32>) {
    e.storage()
        .set::<TokenDataKey, BytesN<32>>(&TokenDataKey::PoolId, pool_id)
}

/***** Asset *****/

pub fn read_asset(e: &Env) -> Asset {
    e.storage()
        .get_unchecked::<TokenDataKey, Asset>(&TokenDataKey::Asset)
        .unwrap()
}

pub fn has_asset(e: &Env) -> bool {
    e.storage().has::<TokenDataKey>(&TokenDataKey::Asset)
}

pub fn write_asset(e: &Env, asset: &Asset) {
    e.storage()
        .set::<TokenDataKey, Asset>(&TokenDataKey::Asset, asset)
}

/***** Decimals *****/

pub fn read_decimals(e: &Env) -> u32 {
    e.storage()
        .get_unchecked::<TokenDataKey, u32>(&TokenDataKey::Decimals)
        .unwrap()
}

pub fn write_decimals(e: &Env, decimals: &u32) {
    e.storage()
        .set::<TokenDataKey, u32>(&TokenDataKey::Decimals, decimals)
}

/***** Name *****/

pub fn read_name(e: &Env) -> Bytes {
    e.storage()
        .get_unchecked::<TokenDataKey, Bytes>(&TokenDataKey::Name)
        .unwrap()
}

pub fn write_name(e: &Env, name: &Bytes) {
    e.storage()
        .set::<TokenDataKey, Bytes>(&TokenDataKey::Name, name)
}

/***** Symbol *****/

pub fn read_symbol(e: &Env) -> Bytes {
    e.storage()
        .get_unchecked::<TokenDataKey, Bytes>(&TokenDataKey::Symbol)
        .unwrap()
}

pub fn write_symbol(e: &Env, symbol: &Bytes) {
    e.storage()
        .set::<TokenDataKey, Bytes>(&TokenDataKey::Symbol, symbol)
}
