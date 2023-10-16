#![cfg(test)]

use soroban_sdk::{Address, Env};

use backstop::{BackstopClient, BackstopContract};

use crate::EmitterContract;

pub(crate) fn create_emitter(e: &Env) -> Address {
    e.register_contract(None, EmitterContract {})
}

pub(crate) fn create_backstop(e: &Env) -> (Address, BackstopClient) {
    let contract_address = e.register_contract(None, BackstopContract {});
    (
        contract_address.clone(),
        BackstopClient::new(e, &contract_address),
    )
}
