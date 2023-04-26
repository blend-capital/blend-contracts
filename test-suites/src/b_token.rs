mod b_token_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/b_token.wasm");
}

pub use b_token_contract::{Client as BlendTokenClient, WASM as B_TOKEN_WASM};
