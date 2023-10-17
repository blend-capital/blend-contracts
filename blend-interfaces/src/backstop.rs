mod wasm {
    soroban_sdk::contractimport!(file = "./wasm/backstop.wasm");
}
pub use wasm::{
    BackstopDataKey, BackstopError, Client as BackstopClient, Contract as Backstop,
    PoolBackstopData, PoolBalance, PoolUserKey, UserBalance, UserEmissionData, Q4W,
    WASM as BackstopWASM,
};
