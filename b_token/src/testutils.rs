#![cfg(any(test, feature = "testutils"))]
use rand::{thread_rng, RngCore};

use soroban_sdk::{BytesN, Env};

// Generics
pub(crate) fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}
