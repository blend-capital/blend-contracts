use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PoolFactoryError {
    AlreadyInitialized = 40,
    InvalidPoolInitArgs = 50,
}
