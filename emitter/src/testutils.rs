#![cfg(test)]

use soroban_sdk::{Address, Env};

use crate::EmitterContract;

pub(crate) fn create_emitter(e: &Env) -> Address {
    e.register_contract(None, EmitterContract {})
}
