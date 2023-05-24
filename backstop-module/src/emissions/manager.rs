use cast::i128;
use soroban_sdk::{Address, Env};

use crate::{constants::BACKSTOP_EPOCH, errors::BackstopError, storage};

pub fn add_to_reward_zone(
    e: &Env,
    to_add: Address,
    to_remove: Address,
) -> Result<(), BackstopError> {
    let mut reward_zone = storage::get_reward_zone(&e);
    let max_rz_len = 10 + (i128(e.ledger().timestamp() - BACKSTOP_EPOCH) >> 23); // bit-shift 23 is ~97 day interval

    if max_rz_len > i128(reward_zone.len()) {
        // there is room in the reward zone. Add whatever
        // TODO: Once there is a defined limit of "backstop minimum", ensure it is reached!
        reward_zone.push_front(to_add.clone());
    } else {
        // don't allow rz modifications within 48 hours of the last distribution
        // if pools don't adopt their distributions, the tokens will be lost
        let next_dist = storage::get_next_dist(&e);
        if next_dist != 0 && e.ledger().timestamp() < next_dist - 5 * 24 * 60 * 60 {
            return Err(BackstopError::BadRequest);
        }

        // attempt to swap the "to_remove"
        // TODO: Once there is a defined limit of "backstop minimum", ensure it is reached!
        if storage::get_pool_tokens(&e, &to_add) <= storage::get_pool_tokens(&e, &to_remove) {
            return Err(BackstopError::InvalidRewardZoneEntry);
        }

        // swap to_add for to_remove
        let to_remove_index = reward_zone.first_index_of(to_remove.clone());
        match to_remove_index {
            Some(idx) => {
                reward_zone.set(idx, to_add.clone());
                storage::set_pool_eps(&e, &to_remove, &0);
            }
            None => return Err(BackstopError::InvalidRewardZoneEntry),
        }
    }

    storage::set_reward_zone(&e, &reward_zone);
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, BytesN, Vec,
    };

    /********** add_to_reward_zone **********/

    #[test]
    fn test_add_to_rz_empty_adds_pool() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            base_reserve: 10,
            network_id: Default::default(),
        });

        let backstop_addr = Address::random(&e);
        let to_add = Address::random(&e);

        e.as_contract(&backstop_addr, || {
            let result = add_to_reward_zone(
                &e,
                to_add.clone(),
                Address::from_contract_id(&BytesN::from_array(&e, &[0u8; 32])),
            );
            match result {
                Ok(_) => {
                    let actual_rz = storage::get_reward_zone(&e);
                    let expected_rz: Vec<Address> = vec![&e, to_add];
                    assert_eq!(actual_rz, expected_rz);
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_add_to_rz_increases_size_over_time() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH + (1 << 23),
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let to_add = Address::random(&e);
        let mut reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage::set_reward_zone(&e, &reward_zone);
            let result = add_to_reward_zone(
                &e,
                to_add.clone(),
                Address::from_contract_id(&BytesN::from_array(&e, &[0u8; 32])),
            );
            match result {
                Ok(_) => {
                    let actual_rz = storage::get_reward_zone(&e);
                    reward_zone.push_front(to_add);
                    assert_eq!(actual_rz, reward_zone);
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_add_to_rz_takes_floor_for_size() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH + (1 << 23) - 1,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let to_add = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage::set_reward_zone(&e, &reward_zone);
            let result = add_to_reward_zone(
                &e,
                to_add.clone(),
                Address::from_contract_id(&BytesN::from_array(&e, &[0u8; 32])),
            );
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::InvalidRewardZoneEntry => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_add_to_rz_swap_happy_path() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let mut reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            to_remove.clone(), // index 7
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_dist(&e, &(BACKSTOP_EPOCH + 5 * 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_tokens(&e, &to_add, &100);
            storage::set_pool_tokens(&e, &to_remove, &99);

            let result = add_to_reward_zone(&e, to_add.clone(), to_remove.clone());
            match result {
                Ok(_) => {
                    let remove_eps = storage::get_pool_eps(&e, &to_remove);
                    assert_eq!(remove_eps, 0);
                    let actual_rz = storage::get_reward_zone(&e);
                    assert_eq!(actual_rz.len(), 10);
                    reward_zone.set(7, to_add);
                    assert_eq!(actual_rz, reward_zone);
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_add_to_rz_swap_not_enough_tokens() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            to_remove.clone(), // index 7
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage::set_reward_zone(&e, &reward_zone.clone());
            storage::set_next_dist(&e, &(BACKSTOP_EPOCH + 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_tokens(&e, &to_add, &100);
            storage::set_pool_tokens(&e, &to_remove, &100);

            let result = add_to_reward_zone(&e, to_add.clone(), to_remove);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::InvalidRewardZoneEntry => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_add_to_rz_to_remove_not_in_rz() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_dist(&e, &(BACKSTOP_EPOCH + 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_tokens(&e, &to_add, &100);
            storage::set_pool_tokens(&e, &to_remove, &99);

            let result = add_to_reward_zone(&e, to_add.clone(), to_remove);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::InvalidRewardZoneEntry => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_add_to_rz_swap_too_soon_to_distribution() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            to_remove.clone(), // index 7
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_dist(&e, &(BACKSTOP_EPOCH + 5 * 24 * 60 * 60 + 1));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_tokens(&e, &to_add, &100);
            storage::set_pool_tokens(&e, &to_remove, &99);

            let result = add_to_reward_zone(&e, to_add, to_remove);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::BadRequest => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }
}
