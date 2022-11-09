mod oracle;
pub use oracle::{OracleClient, OracleError};
#[cfg(any(test, feature = "testutils"))]
pub use oracle::WASM as ORACLE_WASM;