#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Ledger, LedgerInfo},
    vec, Address, BytesN, Env, Symbol,
};

mod common;
use crate::common::{create_backstop, create_token, create_wasm_emitter};

#[test]
fn test_distribute() {
    let e = Env::default();
    e.ledger().set(LedgerInfo {
        timestamp: 10000000,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);

    let (emitter_id, emitter_client) = create_wasm_emitter(&e);
    let emitter = Address::from_contract_id(&e, &emitter_id);

    let (blnd_id, blnd_client) = create_token(&e, &emitter);
    let (backstop_token_id, _) = create_token(&e, &bombadil);
    let (backstop_id, backstop_client) = create_backstop(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    backstop_client.initialize(&backstop_token_id, &blnd_id, &BytesN::<32>::random(&e));

    emitter_client.initialize(&backstop_id, &blnd_id);

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
        e.recorded_top_authorizations()[0],
        (
            backstop.clone(),
            emitter_id.clone(),
            Symbol::new(&e, "distribute"),
            vec![&e]
        )
    );

    let expected_emissions: i128 = ((seconds_passed + 7 * 24 * 60 * 60) * 1_0000000) as i128;
    assert_eq!(result, expected_emissions);
    assert_eq!(blnd_client.balance(&backstop), expected_emissions);

    e.ledger().set(LedgerInfo {
        timestamp: 10000000 + seconds_passed * 2,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let pre_balance = blnd_client.balance(&backstop);
    let result = emitter_client.distribute();

    let expected_emissions: i128 = (seconds_passed * 1_0000000) as i128;
    assert_eq!(result, expected_emissions);
    assert_eq!(
        blnd_client.balance(&backstop),
        expected_emissions + pre_balance
    );
}
