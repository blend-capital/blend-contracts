mod wasm {
    soroban_sdk::contractimport!(file = "./wasm/emitter.wasm");
}
pub use wasm::{
    Client as EmitterClient, Contract as Emitter, EmitterDataKey, EmitterError, WASM as EmitterWASM,
};
