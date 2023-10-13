pub use pool_factory::{PoolFactory, PoolFactoryClient, PoolFactoryError, PoolInitMeta};

mod wasm {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/optimized/pool_factory.wasm"
    );
}
pub use wasm::WASM as PoolFactoryWASM;
