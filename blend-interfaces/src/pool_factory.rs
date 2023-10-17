mod wasm {
    soroban_sdk::contractimport!(file = "./wasm/pool_factory.wasm");
}
pub use wasm::{
    Client as PoolFactoryClient, Contract as PoolFactory, PoolFactoryDataKey, PoolFactoryError,
    PoolInitMeta, WASM as PoolFactoryWASM,
};
