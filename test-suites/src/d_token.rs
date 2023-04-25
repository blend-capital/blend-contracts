mod d_token_contract {
    soroban_sdk::contractimport!(file = "../target/wasm32-unknown-unknown/optimized/d_token.wasm");
}

pub use d_token_contract::WASM as D_TOKEN_WASM;
