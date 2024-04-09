#![no_std]

pub mod backstop {
    soroban_sdk::contractimport!(file = "./wasm/backstop.wasm");
}
pub mod emitter {
    soroban_sdk::contractimport!(file = "./wasm/emitter.wasm");
}
pub mod pool_factory {
    soroban_sdk::contractimport!(file = "./wasm/pool_factory.wasm");
}
pub mod pool {
    soroban_sdk::contractimport!(file = "./wasm/pool.wasm");
}

#[cfg(any(test, feature = "testutils"))]
pub mod testutils;
