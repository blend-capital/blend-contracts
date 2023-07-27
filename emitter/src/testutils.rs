#![cfg(test)]

use soroban_sdk::{Address, Env};

use backstop_module::{BackstopModule, BackstopModuleClient};

pub(crate) fn create_backstop(e: &Env) -> (Address, BackstopModuleClient) {
    let contract_address = e.register_contract(None, BackstopModule {});
    (
        contract_address.clone(),
        BackstopModuleClient::new(e, &contract_address),
    )
}
