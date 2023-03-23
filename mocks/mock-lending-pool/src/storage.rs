use soroban_sdk::{contracttype, Address, Env};

#[derive(Clone)]
#[contracttype]
pub enum MockPoolDataKey {
    Config(Address),
}

pub fn read_config(e: &Env, user: &Address) -> i128 {
    let key = MockPoolDataKey::Config(user.clone());
    e.storage()
        .get::<MockPoolDataKey, i128>(&key)
        .unwrap_or(Ok(0))
        .unwrap()
}

pub fn write_config(e: &Env, user: &Address, config: &i128) {
    let key = MockPoolDataKey::Config(user.clone());
    e.storage().set::<MockPoolDataKey, i128>(&key, &config);
}
