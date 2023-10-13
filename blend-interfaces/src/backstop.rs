pub use backstop::{Backstop, BackstopClient, BackstopError, PoolBackstopData, UserBalance, Q4W};

mod wasm {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/backstop.wasm");
}
pub use wasm::WASM as BackstopWASM;
