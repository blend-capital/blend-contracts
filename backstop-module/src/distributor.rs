use soroban_sdk::{vec, BytesN, Env, Vec};

use crate::{
    errors::BackstopError,
    storage::{BackstopDataStore, StorageManager},
};

const BACKSTOP_EPOCH: u64 = 1441065600; // The approximate deployment date of the backstop module TODO: pick one

/// The reward zone for distribution
pub struct Distributor;

impl Distributor {
    pub fn add_to_reward_zone(
        e: &Env,
        to_add: BytesN<32>,
        to_remove: BytesN<32>,
    ) -> Result<(), BackstopError> {
        let storage = StorageManager::new(e);
        let mut reward_zone = storage.get_reward_zone();
        let max_rz_len = 10 + ((e.ledger().timestamp() - BACKSTOP_EPOCH) >> 23); // bit-shift 23 is ~97 day interval

        if max_rz_len > reward_zone.len() as u64 {
            // there is room in the reward zone. Add whatever
            // TODO: Once there is a defined limit of "backstop minimum", ensure it is reached!
            reward_zone.push_front(to_add);
        } else {
            // don't allow rz modifications within 48 hours of the last distribution
            // if pools don't adopt their distributions, the tokens will be lost
            let next_dist = storage.get_next_dist();
            if next_dist != 0 && e.ledger().timestamp() < next_dist - 5 * 24 * 60 * 60 {
                return Err(BackstopError::BadRequest);
            }

            // attempt to swap the "to_remove"
            // TODO: Once there is a defined limit of "backstop minimum", ensure it is reached!
            if storage.get_pool_tokens(to_add.clone()) <= storage.get_pool_tokens(to_remove.clone())
            {
                return Err(BackstopError::InvalidRewardZoneEntry);
            }

            // swap to_add for to_remove
            let to_remove_index = reward_zone.first_index_of(to_remove.clone());
            match to_remove_index {
                Some(idx) => {
                    reward_zone.insert(idx, to_add);
                    storage.set_pool_eps(to_remove, 0);
                }
                None => return Err(BackstopError::InvalidRewardZoneEntry),
            }
        }

        storage.set_reward_zone(reward_zone);
        Ok(())
    }

    pub fn distribute(e: &Env) -> Result<(), BackstopError> {
        let storage = StorageManager::new(e);

        if e.ledger().timestamp() < storage.get_next_dist() {
            return Err(BackstopError::BadRequest);
        }

        // TODO: Fetch the emission amount from the emitter
        // @dev: cast to u128 to avoid u64 overflow on backstop emissions calc
        let emission: u128 = 500_000_0000000;

        let reward_zone = storage.get_reward_zone();
        let rz_len = reward_zone.len();
        let mut rz_tokens: Vec<u64> = vec![&e];

        // TODO: Potential to assume optimization of backstop token balances ~= RZ tokens
        //       However, linear iteration over the RZ will still occur
        // fetch total tokens of BLND in the reward zone
        let mut total_tokens: u128 = 0;
        for rz_pool_index in 0..rz_len {
            let rz_pool = reward_zone.get(rz_pool_index).unwrap().unwrap();
            let pool_tokens = storage.get_pool_tokens(rz_pool);
            rz_tokens.push_back(pool_tokens);
            total_tokens += pool_tokens as u128;
        }

        // store pools EPS and distribute emissions to backstop depositors
        let backstop_emissions = (emission * 0_7000000) / 1_0000000;
        for rz_pool_index in 0..rz_len {
            let rz_pool = reward_zone.get(rz_pool_index).unwrap().unwrap();
            let cur_pool_tokens = rz_tokens.pop_front_unchecked().unwrap() as u128;
            let share = (cur_pool_tokens * 1_0000000) / total_tokens;

            let pool_eps = (share * 0_3000000) / 1_0000000;
            storage.set_pool_eps(rz_pool.clone(), pool_eps as u64);

            let pool_backstop_emissions = (share * backstop_emissions) / 1_0000000;
            storage.set_pool_tokens(rz_pool, (cur_pool_tokens + pool_backstop_emissions) as u64);
        }

        storage.set_next_dist(e.ledger().timestamp() + 7 * 24 * 60 * 60);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::testutils::generate_contract_id;

    use super::*;
    use soroban_sdk::{
        testutils::{Ledger, LedgerInfo},
        vec,
    };

    /********** add_to_reward_zone **********/

    #[test]
    fn test_add_to_rz_empty_adds_pool() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);

        e.as_contract(&backstop_addr, || {
            let result = Distributor::add_to_reward_zone(
                &e,
                to_add.clone(),
                BytesN::from_array(&e, &[0u8; 32]),
            );
            match result {
                Ok(_) => {
                    let actual_rz = storage.get_reward_zone();
                    let expected_rz: Vec<BytesN<32>> = vec![&e, to_add];
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
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);
        let mut reward_zone: Vec<BytesN<32>> = vec![
            &e,
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage.set_reward_zone(reward_zone.clone());
            let result = Distributor::add_to_reward_zone(
                &e,
                to_add.clone(),
                BytesN::from_array(&e, &[0u8; 32]),
            );
            match result {
                Ok(_) => {
                    let actual_rz = storage.get_reward_zone();
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
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![
            &e,
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage.set_reward_zone(reward_zone.clone());
            let result = Distributor::add_to_reward_zone(
                &e,
                to_add.clone(),
                BytesN::from_array(&e, &[0u8; 32]),
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
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);
        let to_remove = generate_contract_id(&e);
        let mut reward_zone: Vec<BytesN<32>> = vec![
            &e,
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            to_remove.clone(), // index 7
            generate_contract_id(&e),
            generate_contract_id(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage.set_reward_zone(reward_zone.clone());
            storage.set_next_dist(BACKSTOP_EPOCH + 5 * 24 * 60 * 60);
            storage.set_pool_eps(to_remove.clone(), 1);
            storage.set_pool_tokens(to_add.clone(), 100);
            storage.set_pool_tokens(to_remove.clone(), 99);

            let result = Distributor::add_to_reward_zone(&e, to_add.clone(), to_remove.clone());
            match result {
                Ok(_) => {
                    let actual_rz = storage.get_reward_zone();
                    reward_zone.set(7, to_add);
                    assert_eq!(actual_rz, reward_zone);
                    let remove_eps = storage.get_pool_eps(to_remove);
                    assert_eq!(remove_eps, 0);
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
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);
        let to_remove = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![
            &e,
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            to_remove.clone(), // index 7
            generate_contract_id(&e),
            generate_contract_id(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage.set_reward_zone(reward_zone.clone());
            storage.set_next_dist(BACKSTOP_EPOCH + 24 * 60 * 60);
            storage.set_pool_eps(to_remove.clone(), 1);
            storage.set_pool_tokens(to_add.clone(), 100);
            storage.set_pool_tokens(to_remove.clone(), 100);

            let result = Distributor::add_to_reward_zone(&e, to_add.clone(), to_remove);
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
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);
        let to_remove = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![
            &e,
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage.set_reward_zone(reward_zone.clone());
            storage.set_next_dist(BACKSTOP_EPOCH + 24 * 60 * 60);
            storage.set_pool_eps(to_remove.clone(), 1);
            storage.set_pool_tokens(to_add.clone(), 100);
            storage.set_pool_tokens(to_remove.clone(), 99);

            let result = Distributor::add_to_reward_zone(&e, to_add.clone(), to_remove);
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
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);
        let to_remove = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![
            &e,
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            generate_contract_id(&e),
            to_remove.clone(), // index 7
            generate_contract_id(&e),
            generate_contract_id(&e),
        ];

        e.as_contract(&backstop_addr, || {
            storage.set_reward_zone(reward_zone.clone());
            storage.set_next_dist(BACKSTOP_EPOCH + 5 * 24 * 60 * 60 + 1);
            storage.set_pool_eps(to_remove.clone(), 1);
            storage.set_pool_tokens(to_add.clone(), 100);
            storage.set_pool_tokens(to_remove.clone(), 99);

            let result = Distributor::add_to_reward_zone(&e, to_add.clone(), to_remove);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::BadRequest => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    /********** distribute **********/

    #[test]
    fn test_distribute_happy_path() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let pool_1 = generate_contract_id(&e);
        let pool_2 = generate_contract_id(&e);
        let pool_3 = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        e.as_contract(&backstop_addr, || {
            storage.set_next_dist(BACKSTOP_EPOCH);
            storage.set_reward_zone(reward_zone);
            storage.set_pool_tokens(pool_1.clone(), 300_000_0000000);
            storage.set_pool_tokens(pool_2.clone(), 200_000_0000000);
            storage.set_pool_tokens(pool_3.clone(), 500_000_0000000);

            let result = Distributor::distribute(&e);
            match result {
                Ok(_) => {
                    assert_eq!(storage.get_next_dist(), BACKSTOP_EPOCH + 7 * 24 * 60 * 60);
                    assert_eq!(storage.get_pool_tokens(pool_1.clone()), 405_000_0000000);
                    assert_eq!(storage.get_pool_tokens(pool_2.clone()), 270_000_0000000);
                    assert_eq!(storage.get_pool_tokens(pool_3.clone()), 675_000_0000000);
                    assert_eq!(storage.get_pool_eps(pool_1), 0_0900000);
                    assert_eq!(storage.get_pool_eps(pool_2), 0_0600000);
                    assert_eq!(storage.get_pool_eps(pool_3), 0_1500000);
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_distribute_too_early() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let pool_1 = generate_contract_id(&e);
        let pool_2 = generate_contract_id(&e);
        let pool_3 = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        e.as_contract(&backstop_addr, || {
            storage.set_next_dist(BACKSTOP_EPOCH + 1);
            storage.set_reward_zone(reward_zone);
            storage.set_pool_tokens(pool_1.clone(), 300_000_0000000);
            storage.set_pool_tokens(pool_2.clone(), 200_000_0000000);
            storage.set_pool_tokens(pool_3.clone(), 500_000_0000000);

            let result = Distributor::distribute(&e);
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
