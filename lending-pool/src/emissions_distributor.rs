use soroban_auth::Identifier;
use soroban_sdk::Env;

use crate::{
    dependencies::TokenClient,
    errors::PoolError,
    reserve::Reserve,
    storage::{PoolDataStore, ReserveEmissionsData, StorageManager, UserEmissionData},
};

/// Update the emissions information about a reserve token
///
/// ### Arguments
/// * `reserve` - The reserve being updated
/// * `res_token_type` - The reserve token being acted against (0 for d_token / 1 for b_token)
/// * `user` - The user performing an action against the reserve
///
/// ### Errors
/// If the reserve update failed
pub fn update(
    e: &Env,
    reserve: &Reserve,
    res_token_type: u32,
    user: Identifier,
) -> Result<(), PoolError> {
    if let Some(res_emis_data) = update_emission_data(e, reserve, res_token_type)? {
        update_user_emissions(e, reserve, res_token_type, &res_emis_data, user)
    } else {
        // no emissions data for the reserve exists - nothing to update
        Ok(())
    }
}

/// Update only the reserve token emission data
///
/// Returns the new ReserveEmissionData, if any
///
/// ### Arguments
/// * `reserve` - The reserve being updated
/// * `res_token_type` - The reserve token being acted against (0 for d_token / 1 for b_token)
///
/// ### Errors
/// If the reserve update failed
pub fn update_emission_data(
    e: &Env,
    reserve: &Reserve,
    res_token_type: u32,
) -> Result<Option<ReserveEmissionsData>, PoolError> {
    let storage = StorageManager::new(e);

    let res_token_index: u32 = reserve.config.index * 3 + res_token_type;
    let token_emission_config = match storage.get_res_emis_config(res_token_index) {
        Some(res) => res,
        None => return Ok(None), // no emission exist, no update is required
    };
    let token_emission_data = storage.get_res_emis_data(res_token_index).unwrap(); // exists if config is written to

    let total_supply = if res_token_type == 0 {
        reserve.data.d_supply
    } else {
        reserve.data.b_supply
    };

    if token_emission_data.last_time >= token_emission_config.expiration
        || e.ledger().timestamp() == token_emission_data.last_time
        || token_emission_config.eps == 0
        || total_supply == 0
    {
        return Ok(Some(token_emission_data));
    }

    let ledger_timestamp = if e.ledger().timestamp() > token_emission_config.expiration {
        token_emission_config.expiration
    } else {
        e.ledger().timestamp()
    };

    let tokens_issued = (ledger_timestamp - token_emission_data.last_time) as u128
        * 1_0000000
        * token_emission_config.eps as u128;
    let additional_idx = tokens_issued / (total_supply as u128);
    let new_data = ReserveEmissionsData {
        index: (additional_idx as u64) + token_emission_data.index,
        last_time: ledger_timestamp,
    };
    storage.set_res_emis_data(res_token_index, new_data.clone());
    Ok(Some(new_data))
}

fn update_user_emissions(
    e: &Env,
    reserve: &Reserve,
    res_token_type: u32,
    res_emis_data: &ReserveEmissionsData,
    user: Identifier,
) -> Result<(), PoolError> {
    let storage = StorageManager::new(e);
    let res_token_index: u32 = (reserve.config.index * 3) + res_token_type;

    let token_addr = if res_token_type == 0 {
        &reserve.config.d_token
    } else {
        &reserve.config.b_token
    };
    let user_bal = TokenClient::new(&e, token_addr).balance(&user);

    if let Some(user_data) = storage.get_user_emissions(user.clone(), res_token_index) {
        if user_data.index != res_emis_data.index {
            let mut accrual = user_data.accrued;
            if user_bal != 0 {
                let to_accrue = ((user_bal as u128)
                    * ((res_emis_data.index - user_data.index) as u128))
                    / 1_0000000;
                accrual += to_accrue as u64;
            }
            storage.set_user_emissions(
                user.clone(),
                res_token_index,
                UserEmissionData {
                    index: res_emis_data.index,
                    accrued: accrual,
                },
            );
        }
    } else if user_bal == 0 {
        // first time the user registered an action with the asset since emissions were added
        storage.set_user_emissions(
            user.clone(),
            res_token_index,
            UserEmissionData {
                index: res_emis_data.index,
                accrued: 0,
            },
        );
    } else {
        // user had tokens before emissions began, they are due any historical emissions
        let to_accrue = ((user_bal as u128) * (res_emis_data.index as u128)) / 1_0000000;
        storage.set_user_emissions(
            user.clone(),
            res_token_index,
            UserEmissionData {
                index: res_emis_data.index,
                accrued: to_accrue as u64,
            },
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{ReserveConfig, ReserveData, ReserveEmissionsConfig},
        testutils::{create_token_contract, generate_contract_id},
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::testutils::{Accounts, Ledger, LedgerInfo};

    /********** update **********/

    #[test]
    fn test_update_happy_path() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let bombadil = e.accounts().generate_and_create();
        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: res_token_id,
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        let reserve_emission_config = ReserveEmissionsConfig {
            expiration: 1600000000,
            eps: 0_0100000,
        };
        let reserve_emission_data = ReserveEmissionsData {
            index: 2345678,
            last_time: 1500000000,
        };
        let user_emission_data = UserEmissionData {
            index: 1234567,
            accrued: 0_1000000,
        };

        let res_token_type = 0;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        res_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &2_0000000,
        );
        e.as_contract(&pool_id, || {
            storage.set_res_emis_config(res_token_index, reserve_emission_config);
            storage.set_res_emis_data(res_token_index, reserve_emission_data);
            storage.set_user_emissions(samwise_id.clone(), res_token_index, user_emission_data);

            let _result = update(&e, &reserve, res_token_type, samwise_id.clone());

            let new_reserve_emission_data = storage.get_res_emis_data(res_token_index).unwrap();
            let new_user_emission_data = storage
                .get_user_emissions(samwise_id, res_token_index)
                .unwrap();
            assert_eq!(new_reserve_emission_data.last_time, 1501000000);
            assert_eq!(
                new_user_emission_data.index,
                new_reserve_emission_data.index
            );
            assert_eq!(new_user_emission_data.accrued, 400_3222222);
        });
    }

    #[test]
    fn test_update_no_config_ignores() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let bombadil = e.accounts().generate_and_create();
        let (res_token_id, _) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: res_token_id,
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        let res_token_type = 1;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            // no emission information stored

            let result = update(&e, &reserve, res_token_type, samwise_id.clone());
            match result {
                Ok(_) => {
                    assert!(storage.get_res_emis_data(res_token_index).is_none());
                    assert!(storage
                        .get_user_emissions(samwise_id, res_token_index)
                        .is_none());
                }
                Err(_) => assert!(false),
            }
        });
    }

    /********** update emission data **********/

    #[test]
    fn test_update_emission_data_no_config_ignores() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        let res_token_type = 1;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            // no emission information stored

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    assert!(false)
                }
                None => {
                    assert!(storage.get_res_emis_data(res_token_index).is_none());
                    assert!(storage.get_res_emis_config(res_token_index).is_none());
                }
            }
        });
    }

    #[test]
    fn test_update_emission_data_expired_returns_old() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 1,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };
        let reserve_emission_config = ReserveEmissionsConfig {
            expiration: 1600000000,
            eps: 0_0100000,
        };
        let reserve_emission_data = ReserveEmissionsData {
            index: 2345678,
            last_time: 1600000000,
        };

        let res_token_type = 0;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_res_emis_config(res_token_index, reserve_emission_config);
            storage.set_res_emis_data(res_token_index, reserve_emission_data.clone());

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage.get_res_emis_data(res_token_index).unwrap();
                    assert_eq!(
                        new_reserve_emission_data.last_time,
                        reserve_emission_data.last_time
                    );
                    assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
                }
                None => assert!(false),
            }
        });
    }

    #[test]
    fn test_update_emission_data_updated_this_block_returns_old() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };
        let reserve_emission_config = ReserveEmissionsConfig {
            expiration: 1600000000,
            eps: 0_0100000,
        };
        let reserve_emission_data = ReserveEmissionsData {
            index: 2345678,
            last_time: 1501000000,
        };

        let res_token_type = 1;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_res_emis_config(res_token_index, reserve_emission_config);
            storage.set_res_emis_data(res_token_index, reserve_emission_data.clone());

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage.get_res_emis_data(res_token_index).unwrap();
                    assert_eq!(
                        new_reserve_emission_data.last_time,
                        reserve_emission_data.last_time
                    );
                    assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
                }
                None => assert!(false),
            }
        });
    }

    #[test]
    fn test_update_emission_data_no_eps_returns_old() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 1,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };
        let reserve_emission_config = ReserveEmissionsConfig {
            expiration: 1600000000,
            eps: 0,
        };
        let reserve_emission_data = ReserveEmissionsData {
            index: 2345678,
            last_time: 1500000000,
        };

        let res_token_type = 0;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_res_emis_config(res_token_index, reserve_emission_config);
            storage.set_res_emis_data(res_token_index, reserve_emission_data.clone());

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage.get_res_emis_data(res_token_index).unwrap();
                    assert_eq!(
                        new_reserve_emission_data.last_time,
                        reserve_emission_data.last_time
                    );
                    assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
                }
                None => assert!(false),
            }
        });
    }

    #[test]
    fn test_update_emission_data_no_supply_returns_old() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 1,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 0,
                d_supply: 100_0000000,
                last_block: 123,
            },
        };
        let reserve_emission_config = ReserveEmissionsConfig {
            expiration: 1600000000,
            eps: 0_0100000,
        };
        let reserve_emission_data = ReserveEmissionsData {
            index: 2345678,
            last_time: 1500000000,
        };

        let res_token_type = 1;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_res_emis_config(res_token_index, reserve_emission_config);
            storage.set_res_emis_data(res_token_index, reserve_emission_data.clone());

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage.get_res_emis_data(res_token_index).unwrap();
                    assert_eq!(
                        new_reserve_emission_data.last_time,
                        reserve_emission_data.last_time
                    );
                    assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
                }
                None => assert!(false),
            }
        });
    }

    #[test]
    fn test_update_emission_data_d_token_past_exp() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1700000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 2,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 0,
                d_supply: 100_0000000,
                last_block: 123,
            },
        };
        let reserve_emission_config = ReserveEmissionsConfig {
            expiration: 1600000001,
            eps: 0_0100000,
        };
        let reserve_emission_data = ReserveEmissionsData {
            index: 123456789,
            last_time: 1500000000,
        };

        let res_token_type = 0;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_res_emis_config(res_token_index, reserve_emission_config);
            storage.set_res_emis_data(res_token_index, reserve_emission_data.clone());

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage.get_res_emis_data(res_token_index).unwrap();
                    assert_eq!(new_reserve_emission_data.last_time, 1600000001);
                    assert_eq!(new_reserve_emission_data.index, 10012_3457789);
                }
                None => assert!(false),
            }
        });
    }

    #[test]
    fn test_update_emission_data_b_token_rounds_down() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000005,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 2,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0001111,
                d_supply: 0,
                last_block: 123,
            },
        };
        let reserve_emission_config = ReserveEmissionsConfig {
            expiration: 1600000000,
            eps: 0_0100000,
        };
        let reserve_emission_data = ReserveEmissionsData {
            index: 123456789,
            last_time: 1500000000,
        };

        let res_token_type = 1;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_res_emis_config(res_token_index, reserve_emission_config);
            storage.set_res_emis_data(res_token_index, reserve_emission_data.clone());

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage.get_res_emis_data(res_token_index).unwrap();
                    assert_eq!(new_reserve_emission_data.last_time, 1500000005);
                    assert_eq!(new_reserve_emission_data.index, 123461788);
                }
                None => assert!(false),
            }
        });
    }

    /********** update_user_emissions **********/

    #[test]
    fn test_update_user_emissions_first_time() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let bombadil = e.accounts().generate_and_create();
        let (res_token_id, _res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: res_token_id,
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        let reserve_emission_data = ReserveEmissionsData {
            index: 123456789,
            last_time: 1500000000,
        };

        let res_token_type = 0;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                samwise_id.clone(),
            )
            .unwrap();

            let new_user_emission_data = storage
                .get_user_emissions(samwise_id, res_token_index)
                .unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0);
        });
    }

    #[test]
    fn test_update_user_emissions_first_time_had_tokens() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let bombadil = e.accounts().generate_and_create();
        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: res_token_id,
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 1,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 100_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        // if let Some(user_data) = storage.get_user_emissions(user.clone(), res_token_index) {
        //     if user_bal != 0 && user_data.index != res_emis_data.index {
        //         let to_accrue = ((user_bal as u128) * ((res_emis_data.index - user_data.index) as u128)) / 1_0000000;
        //         storage.set_user_emissions(user.clone(), res_token_index, UserEmissionData {
        //             index: res_emis_data.index,
        //             accrued: to_accrue as u64 + user_data.accrued
        //         });
        //     }
        // } else if user_bal == 0 {
        //     // first time the user registered an action with the asset since emissions were added
        //     storage.set_user_emissions(user.clone(), res_token_index, UserEmissionData {
        //         index: res_emis_data.index,
        //         accrued: 0
        //     });
        // }

        let reserve_emission_data = ReserveEmissionsData {
            index: 123456789,
            last_time: 1500000000,
        };

        res_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &0_5000000,
        );
        let res_token_type = 0;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                samwise_id.clone(),
            )
            .unwrap();

            let new_user_emission_data = storage
                .get_user_emissions(samwise_id, res_token_index)
                .unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 6_1728394);
        });
    }

    #[test]
    fn test_update_user_emissions_no_bal_no_accrual() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let bombadil = e.accounts().generate_and_create();
        let (res_token_id, _res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: res_token_id,
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 60_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        // if let Some(user_data) = storage.get_user_emissions(user.clone(), res_token_index) {
        //     if --user_bal != 0-- && user_data.index != res_emis_data.index {
        //         let to_accrue = ((user_bal as u128) * ((res_emis_data.index - user_data.index) as u128)) / 1_0000000;
        //         storage.set_user_emissions(user.clone(), res_token_index, UserEmissionData {
        //             index: res_emis_data.index,
        //             accrued: to_accrue as u64 + user_data.accrued
        //         });
        //     }

        let reserve_emission_data = ReserveEmissionsData {
            index: 123456789,
            last_time: 1500000000,
        };
        let user_emission_data = UserEmissionData {
            index: 56789,
            accrued: 0_1000000,
        };

        // res_token_client.with_source_account(&bombadil).mint(
        //     &Signature::Invoker,
        //     &0,
        //     &samwise_id,
        //     &0_5000000,
        // );
        let res_token_type = 1;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_user_emissions(samwise_id.clone(), res_token_index, user_emission_data);

            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                samwise_id.clone(),
            )
            .unwrap();

            let new_user_emission_data = storage
                .get_user_emissions(samwise_id, res_token_index)
                .unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0_1000000);
        });
    }

    #[test]
    fn test_update_user_emissions_if_accrued_skips() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let bombadil = e.accounts().generate_and_create();
        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: res_token_id,
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 1,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 60_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        let reserve_emission_data = ReserveEmissionsData {
            index: 123456789,
            last_time: 1500000000,
        };
        let user_emission_data = UserEmissionData {
            index: 123456789,
            accrued: 1_1000000,
        };

        res_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &0_5000000,
        );
        let res_token_type = 0;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_user_emissions(
                samwise_id.clone(),
                res_token_index,
                user_emission_data.clone(),
            );

            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                samwise_id.clone(),
            )
            .unwrap();

            let new_user_emission_data = storage
                .get_user_emissions(samwise_id, res_token_index)
                .unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, user_emission_data.accrued);
        });
    }

    #[test]
    fn test_update_user_emissions_accrues() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let bombadil = e.accounts().generate_and_create();
        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: res_token_id,
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000,
                index: 1,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 60_0000000,
                d_supply: 50_0000000,
                last_block: 123,
            },
        };

        let reserve_emission_data = ReserveEmissionsData {
            index: 123456789,
            last_time: 1500000000,
        };
        let user_emission_data = UserEmissionData {
            index: 56789,
            accrued: 0_1000000,
        };

        res_token_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &samwise_id,
            &0_5000000,
        );
        let res_token_type = 1;
        let res_token_index = reserve.config.index * 3 + res_token_type;
        e.as_contract(&pool_id, || {
            storage.set_user_emissions(
                samwise_id.clone(),
                res_token_index,
                user_emission_data.clone(),
            );

            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                samwise_id.clone(),
            )
            .unwrap();

            let new_user_emission_data = storage
                .get_user_emissions(samwise_id, res_token_index)
                .unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 6_2700000);
        });
    }
}
