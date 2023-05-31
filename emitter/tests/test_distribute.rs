#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    vec, Address, Env, Symbol,
};

mod common;
use crate::common::{create_backstop, create_token, create_wasm_emitter};

#[test]
fn test_distribute() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().set(LedgerInfo {
        timestamp: 10000000,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);

    let (emitter_address, emitter_client) = create_wasm_emitter(&e);

    let (blnd_id, blnd_client) = create_token(&e, &emitter_address);
    let (backstop_token_id, _) = create_token(&e, &bombadil);
    let (backstop_address, backstop_client) = create_backstop(&e);
    backstop_client.initialize(&backstop_token_id, &blnd_id, &Address::random(&e));

    emitter_client.initialize(&backstop_address, &blnd_id);

    let seconds_passed = 12345;
    e.ledger().set(LedgerInfo {
        timestamp: 10000000 + seconds_passed,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let result = emitter_client.distribute();
    assert_eq!(
        e.auths()[0],
        (
            backstop_address.clone(),
            emitter_address.clone(),
            Symbol::new(&e, "distribute"),
            vec![&e]
        )
    );

    let expected_emissions: i128 = ((seconds_passed + 7 * 24 * 60 * 60) * 1_0000000) as i128;
    assert_eq!(result, expected_emissions);
    assert_eq!(blnd_client.balance(&backstop_address), expected_emissions);

    e.ledger().set(LedgerInfo {
        timestamp: 10000000 + seconds_passed * 2,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let pre_balance = blnd_client.balance(&backstop_address);
    let result = emitter_client.distribute();

    let expected_emissions: i128 = (seconds_passed * 1_0000000) as i128;
    assert_eq!(result, expected_emissions);
    assert_eq!(
        blnd_client.balance(&backstop_address),
        expected_emissions + pre_balance
    );
}
