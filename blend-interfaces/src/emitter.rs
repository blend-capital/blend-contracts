pub use emitter::{Emitter, EmitterClient, EmitterContract, EmitterError};

mod wasm {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/emitter.wasm");
}
pub use wasm::WASM as EmitterWASM;
