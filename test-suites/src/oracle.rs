mod oracle_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/oracle.wasm");
}
pub use oracle_contract::{
    BlendOracleDataKey, Client as OracleClient, OracleError, WASM as ORACLE_WASM,
};
