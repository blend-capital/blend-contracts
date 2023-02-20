use cast::i128;
use fixed_point_math::{FixedPoint, STROOP};
use soroban_sdk::{symbol, vec, BytesN, Env, Vec};

use crate::{constants::SCALAR_7, errors::BackstopError, storage};

const BACKSTOP_EPOCH: u64 = 1441065600; // The approximate deployment date of the backstop module TODO: pick one

pub fn add_to_reward_zone(
    e: &Env,
    to_add: BytesN<32>,
    to_remove: BytesN<32>,
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
                reward_zone.insert(idx, to_add.clone());
                storage::set_pool_eps(&e, &to_remove, &0);
            }
            None => return Err(BackstopError::InvalidRewardZoneEntry),
        }
    }

    storage::set_reward_zone(&e, &reward_zone);
    Ok(())
}

pub fn distribute(e: &Env) -> Result<(), BackstopError> {
    if e.ledger().timestamp() < storage::get_next_dist(&e) {
        return Err(BackstopError::BadRequest);
    }

    // TODO: Fetch the emission amount from the emitter
    let emission: i128 = 500_000_0000000;

    let reward_zone = storage::get_reward_zone(&e);
    let rz_len = reward_zone.len();
    let mut rz_tokens: Vec<i128> = vec![&e];

    // TODO: Potential to assume optimization of backstop token balances ~= RZ tokens
    //       However, linear iteration over the RZ will still occur
    // fetch total tokens of BLND in the reward zone
    let mut total_tokens: i128 = 0;
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone.get(rz_pool_index).unwrap().unwrap();
        let pool_tokens = storage::get_pool_tokens(&e, &rz_pool);
        rz_tokens.push_back(pool_tokens);
        total_tokens += i128(pool_tokens);
    }

    // store pools EPS and distribute emissions to backstop depositors
    let backstop_emissions = emission.fixed_mul_floor(0_7000000, SCALAR_7).unwrap();
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone.get(rz_pool_index).unwrap().unwrap();
        let cur_pool_tokens = i128(rz_tokens.pop_front_unchecked().unwrap());
        let share = cur_pool_tokens
            .fixed_div_floor(total_tokens, SCALAR_7)
            .unwrap();

        // store pool EPS and distribute pool's emissions
        let pool_eps = share.fixed_mul_floor(0_3000000, SCALAR_7).unwrap();
        let pool_emissions = storage::get_pool_emis(&e, &rz_pool) + (pool_eps * 7 * 24 * 60 * 60);
        storage::set_pool_eps(&e, &rz_pool, &pool_eps);
        storage::set_pool_emis(&e, &rz_pool, &pool_emissions);

        // distribute backstop depositor emissions
        let pool_backstop_emissions = share.fixed_mul_floor(backstop_emissions, SCALAR_7).unwrap();
        storage::set_pool_tokens(&e, &rz_pool, &(cur_pool_tokens + pool_backstop_emissions));
    }

    storage::set_next_dist(&e, &(e.ledger().timestamp() + 7 * 24 * 60 * 60));

    Ok(())
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
            base_reserve: 10,
            network_id: Default::default(),
        });

        let backstop_addr = generate_contract_id(&e);
        let to_add = generate_contract_id(&e);

        e.as_contract(&backstop_addr, || {
            let result = add_to_reward_zone(&e, to_add.clone(), BytesN::from_array(&e, &[0u8; 32]));
            match result {
                Ok(_) => {
                    let actual_rz = storage::get_reward_zone(&e);
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
            network_id: Default::default(),
            base_reserve: 10,
        });

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
            storage::set_reward_zone(&e, &reward_zone);
            let result = add_to_reward_zone(&e, to_add.clone(), BytesN::from_array(&e, &[0u8; 32]));
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
            storage::set_reward_zone(&e, &reward_zone);
            let result = add_to_reward_zone(&e, to_add.clone(), BytesN::from_array(&e, &[0u8; 32]));
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
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_dist(&e, &(BACKSTOP_EPOCH + 5 * 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_tokens(&e, &to_add, &100);
            storage::set_pool_tokens(&e, &to_remove, &99);

            let result = add_to_reward_zone(&e, to_add.clone(), to_remove.clone());
            match result {
                Ok(_) => {
                    let actual_rz = storage::get_reward_zone(&e);
                    reward_zone.set(7, to_add);
                    assert_eq!(actual_rz, reward_zone);
                    let remove_eps = storage::get_pool_eps(&e, &to_remove);
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
            network_id: Default::default(),
            base_reserve: 10,
        });

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

    /********** distribute **********/

    #[test]
    fn test_distribute_happy_path() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = generate_contract_id(&e);
        let pool_1 = generate_contract_id(&e);
        let pool_2 = generate_contract_id(&e);
        let pool_3 = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        e.as_contract(&backstop_addr, || {
            storage::set_next_dist(&e, &BACKSTOP_EPOCH);
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_pool_tokens(&e, &pool_1, &300_000_0000000);
            storage::set_pool_tokens(&e, &pool_2, &200_000_0000000);
            storage::set_pool_tokens(&e, &pool_3, &500_000_0000000);
            storage::set_pool_emis(&e, &pool_1, &100_123_0000000);

            let result = Distributor::distribute(&e);
            match result {
                Ok(_) => {
                    assert_eq!(
                        storage::get_next_dist(&e),
                        BACKSTOP_EPOCH + 7 * 24 * 60 * 60
                    );
                    assert_eq!(storage::get_pool_tokens(&e, &pool_1), 405_000_0000000);
                    assert_eq!(storage::get_pool_tokens(&e, &pool_2), 270_000_0000000);
                    assert_eq!(storage::get_pool_tokens(&e, &pool_3), 675_000_0000000);
                    assert_eq!(storage::get_pool_eps(&e, &pool_1), 0_0900000);
                    assert_eq!(storage::get_pool_eps(&e, &pool_2), 0_0600000);
                    assert_eq!(storage::get_pool_eps(&e, &pool_3), 0_1500000);
                    assert_eq!(storage::get_pool_emis(&e, &pool_1), 154_555_0000000);
                    assert_eq!(storage::get_pool_emis(&e, &pool_2), 36_288_0000000);
                    assert_eq!(storage::get_pool_emis(&e, &pool_3), 90_720_0000000);
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
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = generate_contract_id(&e);
        let pool_1 = generate_contract_id(&e);
        let pool_2 = generate_contract_id(&e);
        let pool_3 = generate_contract_id(&e);
        let reward_zone: Vec<BytesN<32>> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        e.as_contract(&backstop_addr, || {
            storage::set_next_dist(&e, &(BACKSTOP_EPOCH + 1));
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_pool_tokens(&e, &pool_1, &300_000_0000000);
            storage::set_pool_tokens(&e, &pool_2, &200_000_0000000);
            storage::set_pool_tokens(&e, &pool_3, &500_000_0000000);

            let result = distribute(&e);
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
