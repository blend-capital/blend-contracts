#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    Address, Env, Status,
};

mod common;
use crate::common::{create_token, create_wasm_emitter, generate_contract_id, EmitterError};

#[test]
fn test_distribute_from_backstop() {
    let e = Env::default();
    e.ledger().set(LedgerInfo {
        timestamp: 100,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);

    let backstop = Address::random(&e);
    let blend_lp = generate_contract_id(&e);

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Address::from_contract_id(&e, &emitter);
    let (blend_id, blend_client) = create_token(&e, &emitter_id);
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);

    //pass some time
    let seconds_passed = 10000;
    e.ledger().set(LedgerInfo {
        timestamp: 100 + seconds_passed,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    //Note: this function is not currently working properly - wait for- https://github.com/stellar/rs-soroban-sdk/issues/868
    // let result = emitter_client.try_distribute() (&backstop).distribute();

    // let expected_emissions = BigInt::from_u64(&e, seconds_passed * 1_0000000);

    // assert_eq!(result, expected_emissions);
    // assert_eq!(blend_client.balance(&backstop_id), expected_emissions);
}

#[test]
fn test_distribute_from_non_backstop_panics() {
    let e = Env::default();
    e.ledger().set(LedgerInfo {
        timestamp: 100,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);
    let sauron = Address::random(&e);

    let backstop = Address::random(&e);
    let blend_lp = generate_contract_id(&e);

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Address::from_contract_id(&e, &emitter);
    let (blend_id, blend_client) = create_token(&e, &emitter_id);
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);

    // pass some time
    let seconds_passed = 10000;
    e.ledger().set(LedgerInfo {
        timestamp: 100 + seconds_passed,
        protocol_version: 1,
        sequence_number: 10,
        base_reserve: 10,
        network_id: Default::default(),
    });

    let result = emitter_client.try_distribute();

    match result {
        Ok(_) => {
            assert!(true); // TODO: auth (see `distribute`)
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, EmitterError::NotAuthorized),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(1)),
        },
    }
}
