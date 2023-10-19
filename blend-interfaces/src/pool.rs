mod wasm {
    soroban_sdk::contractimport!(file = "./wasm/pool.wasm");
}
pub use wasm::{
    AuctionData, AuctionKey, Client as PoolClient, Contract as Pool, PoolConfig, PoolDataKey,
    PoolError, Positions, Request, ReserveConfig, ReserveData, ReserveEmissionMetadata,
    ReserveEmissionsConfig, ReserveEmissionsData, UserEmissionData, UserReserveKey,
    WASM as PoolWASM,
};
