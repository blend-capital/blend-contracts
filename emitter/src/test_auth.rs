#![cfg(test)]
extern crate std;
use crate::{contract::EmitterContractClient, dependencies::TokenClient};

use super::*;

use soroban_sdk::{
    testutils::{BytesN as _, Ledger, LedgerInfo},
    vec, Address, BytesN, Env, Symbol,
};

#[test]
fn test_distribute_requires_auth() {
    let e = Env::default();
    e.ledger().set(LedgerInfo {
        timestamp: 100,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let emitter_id = e.register_contract(None, EmitterContract);
    let emitter_client = EmitterContractClient::new(&e, &emitter_id);
    let emitter = Address::from_contract_id(&e, &emitter_id);

    let blnd_id = e.register_stellar_asset_contract(emitter.clone());
    let blnd_client = TokenClient::new(&e, &blnd_id);

    let backstop_id = BytesN::<32>::random(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);

    emitter_client.initialize(&backstop_id, &blnd_id);

    let seconds_passed = 12345;
    e.ledger().set(LedgerInfo {
        timestamp: 100 + seconds_passed,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let result = emitter_client.distribute();
    let authorizations = e.recorded_top_authorizations();

    let expected_emissions: i128 = (seconds_passed * 1_0000000) as i128;
    assert_eq!(result, expected_emissions);
    assert_eq!(blnd_client.balance(&backstop), expected_emissions);

    // verify the backstop was authed
    assert_eq!(
        authorizations[0],
        (
            // Address for which auth is performed
            backstop.clone(),
            // Identifier of the called contract
            emitter_id.clone(),
            // Name of the called function
            Symbol::new(&e, "distribute"),
            vec![&e]
        )
    );
}
