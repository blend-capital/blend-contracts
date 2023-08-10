#![cfg(test)]
extern crate std;
use std::vec as std_vec;

use crate::{contract::EmitterClient, dependencies::TokenClient};

use super::*;

use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, Ledger, LedgerInfo},
    vec, Address, Env, Symbol,
};

#[test]
fn test_distribute_requires_auth() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().set(LedgerInfo {
        timestamp: 100000000,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_expiration: 10,
        min_persistent_entry_expiration: 10,
        max_entry_expiration: 2000000,
    });

    let emitter_address = e.register_contract(None, Emitter {});
    let emitter_client = EmitterClient::new(&e, &emitter_address);

    let blnd_id = e.register_stellar_asset_contract(emitter_address.clone());
    let blnd_client = TokenClient::new(&e, &blnd_id);

    let backstop_address = Address::random(&e);

    emitter_client.initialize(&backstop_address, &blnd_id);

    let seconds_passed = 12345;
    e.ledger().set(LedgerInfo {
        timestamp: 100000000 + seconds_passed,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_expiration: 10,
        min_persistent_entry_expiration: 10,
        max_entry_expiration: 2000000,
    });

    let result = emitter_client.distribute();
    let authorizations = e.auths();

    let expected_emissions: i128 = ((seconds_passed + 7 * 24 * 60 * 60) * 1_0000000) as i128;
    assert_eq!(result, expected_emissions);
    assert_eq!(blnd_client.balance(&backstop_address), expected_emissions);

    // verify the backstop was authed
    assert_eq!(
        authorizations[0],
        (
            // Address for which auth is performed
            backstop_address.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    emitter_address.clone(),
                    Symbol::new(&e, "distribute"),
                    vec![&e]
                )),
                sub_invocations: std_vec![],
            }
        )
    );
}
