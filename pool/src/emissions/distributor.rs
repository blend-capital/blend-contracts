use cast::i128;
use sep_41_token::TokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, Address, Env, Vec};

use crate::{
    errors::PoolError,
    pool::User,
    storage::{self, ReserveEmissionsData, UserEmissionData},
    ReserveEmissionsConfig,
};

/// Performs a claim against the given "reserve_token_ids" for "from"
pub fn execute_claim(e: &Env, from: &Address, reserve_token_ids: &Vec<u32>, to: &Address) -> i128 {
    let from_state = User::load(e, from);
    let reserve_list = storage::get_res_list(e);
    let mut to_claim = 0;
    for reserve_token_id in reserve_token_ids.clone() {
        let reserve_index = reserve_token_id / 2;
        let reserve_addr = reserve_list.get(reserve_index);
        match reserve_addr {
            Some(res_address) => {
                let reserve_config = storage::get_res_config(e, &res_address);
                let reserve_data = storage::get_res_data(e, &res_address);
                let (user_balance, supply) = match reserve_token_id % 2 {
                    0 => (
                        from_state.get_liabilities(reserve_index),
                        reserve_data.d_supply,
                    ),
                    1 => (
                        from_state.get_total_supply(reserve_index),
                        reserve_data.b_supply,
                    ),
                    _ => panic_with_error!(e, PoolError::BadRequest),
                };
                to_claim += update_emissions(
                    e,
                    reserve_token_id,
                    supply,
                    10i128.pow(reserve_config.decimals),
                    from,
                    user_balance,
                    true,
                );
            }
            None => {
                panic_with_error!(e, PoolError::BadRequest)
            }
        }
    }

    if to_claim > 0 {
        let backstop = storage::get_backstop(e);
        let blnd_token = storage::get_blnd_token(e);
        TokenClient::new(e, &blnd_token).transfer_from(
            &e.current_contract_address(),
            &backstop,
            to,
            &to_claim,
        );
    }
    to_claim
}

/// Update the emissions information about a reserve token. Must be called before any update
/// is made to the supply of debtTokens or blendTokens.
///
/// Returns the amount of tokens to claim, or zero if 'claim' is false
///
/// ### Arguments
/// * `res_token_id` - The reserve token being acted against => (reserve index * 2 + (0 for debtToken or 1 for blendToken))
/// * `supply` - The current supply of the reserve token
/// * `supply_scalar` - The scalar of the reserve token
/// * `user` - The user performing an action against the reserve
/// * `balance` - The current balance of the user
/// * `claim` - Whether or not to claim the user's accrued emissions
///
/// ### Panics
/// If the reserve update failed
pub fn update_emissions(
    e: &Env,
    res_token_id: u32,
    supply: i128,
    supply_scalar: i128,
    user: &Address,
    balance: i128,
    claim: bool,
) -> i128 {
    if let Some(res_emis_data) = update_emission_data(e, res_token_id, supply, supply_scalar) {
        return update_user_emissions(
            e,
            &res_emis_data,
            res_token_id,
            supply_scalar,
            user,
            balance,
            claim,
        );
    }
    // no emissions data for the reserve exists - nothing to update
    0
}

/// Update the reserve token emission data
///
/// Returns the new ReserveEmissionData, if None if no data exists
///
/// ### Arguments
/// * `res_token_id` - The reserve token being acted against => (reserve index * 2 + (0 for debtToken or 1 for blendToken))
/// * `supply` - The current supply of the reserve token
/// * `supply_scalar` - The scalar of the reserve token
///
/// ### Panics
/// If the reserve update failed
pub fn update_emission_data(
    e: &Env,
    res_token_id: u32,
    supply: i128,
    supply_scalar: i128,
) -> Option<ReserveEmissionsData> {
    match storage::get_res_emis_config(e, &res_token_id) {
        Some(emis_config) => Some(update_emission_data_with_config(
            e,
            res_token_id,
            supply,
            supply_scalar,
            &emis_config,
        )),
        None => return None, // no emission exist, no update is required
    }
}

pub(super) fn update_emission_data_with_config(
    e: &Env,
    res_token_id: u32,
    supply: i128,
    supply_scalar: i128,
    emis_config: &ReserveEmissionsConfig,
) -> ReserveEmissionsData {
    let token_emission_data = storage::get_res_emis_data(e, &res_token_id).unwrap_optimized(); // exists if config is written to

    if token_emission_data.last_time >= emis_config.expiration
        || e.ledger().timestamp() == token_emission_data.last_time
        || emis_config.eps == 0
        || supply == 0
    {
        return token_emission_data;
    }

    let ledger_timestamp = if e.ledger().timestamp() > emis_config.expiration {
        emis_config.expiration
    } else {
        e.ledger().timestamp()
    };

    let additional_idx = (i128(ledger_timestamp - token_emission_data.last_time)
        * i128(emis_config.eps))
    .fixed_div_floor(supply, supply_scalar)
    .unwrap_optimized();
    let new_data = ReserveEmissionsData {
        index: additional_idx + token_emission_data.index,
        last_time: ledger_timestamp,
    };
    storage::set_res_emis_data(e, &res_token_id, &new_data);
    new_data
}

fn update_user_emissions(
    e: &Env,
    res_emis_data: &ReserveEmissionsData,
    res_token_id: u32,
    supply_scalar: i128,
    user: &Address,
    balance: i128,
    claim: bool,
) -> i128 {
    if let Some(user_data) = storage::get_user_emissions(e, user, &res_token_id) {
        if user_data.index != res_emis_data.index || claim {
            let mut accrual = user_data.accrued;
            if balance != 0 {
                let to_accrue = balance
                    .fixed_mul_floor(res_emis_data.index - user_data.index, supply_scalar)
                    .unwrap_optimized();
                accrual += to_accrue;
            }
            return set_user_emissions(e, user, res_token_id, res_emis_data.index, accrual, claim);
        }
        0
    } else if balance == 0 {
        // first time the user registered an action with the asset since emissions were added
        return set_user_emissions(e, user, res_token_id, res_emis_data.index, 0, claim);
    } else {
        // user had tokens before emissions began, they are due any historical emissions
        let to_accrue = balance
            .fixed_mul_floor(res_emis_data.index, supply_scalar)
            .unwrap_optimized();
        return set_user_emissions(e, user, res_token_id, res_emis_data.index, to_accrue, claim);
    }
}

fn set_user_emissions(
    e: &Env,
    user: &Address,
    res_token_id: u32,
    index: i128,
    accrued: i128,
    claim: bool,
) -> i128 {
    if claim {
        storage::set_user_emissions(
            e,
            user,
            &res_token_id,
            &UserEmissionData { index, accrued: 0 },
        );
        accrued
    } else {
        storage::set_user_emissions(e, user, &res_token_id, &UserEmissionData { index, accrued });
        0
    }
}

#[cfg(test)]
mod tests {
    use crate::{pool::Positions, storage::ReserveEmissionsConfig, testutils};

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
        vec,
    };

    /********** update_emissions **********/

    #[test]
    fn test_update_emissions() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);
        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply: i128 = 50_0000000;
        let user_position: i128 = 2_0000000;
        e.as_contract(&pool, || {
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
            let res_token_index = 1 * 2 + res_token_type;

            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            let _result = update_emissions(
                &e,
                res_token_index,
                supply,
                1_0000000,
                &samwise,
                user_position,
                false,
            );

            let new_reserve_emission_data =
                storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_reserve_emission_data.last_time, 1501000000);
            assert_eq!(
                new_user_emission_data.index,
                new_reserve_emission_data.index
            );
            assert_eq!(new_user_emission_data.accrued, 400_3222222);
        });
    }

    #[test]
    fn test_update_emissions_no_config_ignores() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);
        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply: i128 = 100_0000000;
        let user_position: i128 = 2_0000000;
        e.as_contract(&pool, || {
            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;

            let result = update_emissions(
                &e,
                res_token_index,
                supply,
                1_0000000,
                &samwise,
                user_position,
                false,
            );
            if result == 0 {
                assert!(storage::get_res_emis_data(&e, &res_token_index).is_none());
                assert!(storage::get_user_emissions(&e, &samwise, &res_token_index).is_none());
            } else {
                assert!(false);
            }
        });
    }

    /********** update emission data **********/

    #[test]
    fn test_update_emission_data_no_config_returns_none() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply = 50_0000000;
        let supply_scalar = 1_0000000;
        e.as_contract(&pool, || {
            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;

            // no emission information stored

            let result = update_emission_data(&e, res_token_index, supply, supply_scalar);
            match result {
                Some(_) => {
                    assert!(false)
                }
                None => {
                    assert!(storage::get_res_emis_data(&e, &res_token_index).is_none());
                    assert!(storage::get_res_emis_config(&e, &res_token_index).is_none());
                }
            }
        });
    }

    #[test]
    fn test_update_emission_data_expired_returns_old() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1601000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply = 50_0000000;
        let supply_scalar = 1_0000000;
        e.as_contract(&pool, || {
            let reserve_emission_config = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0100000,
            };
            let reserve_emission_data = ReserveEmissionsData {
                index: 2345678,
                last_time: 1600000000,
            };

            let res_token_type = 0;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, res_token_index, supply, supply_scalar);
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
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
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply = 50_0000000;
        let supply_scalar = 1_0000000;
        e.as_contract(&pool, || {
            let reserve_emission_config = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0100000,
            };
            let reserve_emission_data = ReserveEmissionsData {
                index: 2345678,
                last_time: 1501000000,
            };

            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, res_token_index, supply, supply_scalar);
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
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
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply = 50_0000000;
        let supply_scalar = 1_0000000;
        e.as_contract(&pool, || {
            let reserve_emission_config = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0,
            };
            let reserve_emission_data = ReserveEmissionsData {
                index: 2345678,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, res_token_index, supply, supply_scalar);
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
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
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply = 0;
        let supply_scalar = 1_0000000;
        e.as_contract(&pool, || {
            let reserve_emission_config = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0100000,
            };
            let reserve_emission_data = ReserveEmissionsData {
                index: 2345678,
                last_time: 1500000000,
            };

            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, res_token_index, supply, supply_scalar);
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
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
    fn test_update_emission_data_past_exp() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1700000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply = 100_0000000;
        let supply_scalar = 1_0000000;
        e.as_contract(&pool, || {
            let reserve_emission_config = ReserveEmissionsConfig {
                expiration: 1600000001,
                eps: 0_0100000,
            };
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, res_token_index, supply, supply_scalar);
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
                    assert_eq!(new_reserve_emission_data.last_time, 1600000001);
                    assert_eq!(new_reserve_emission_data.index, 10012_3457789);
                }
                None => assert!(false),
            }
        });
    }

    #[test]
    fn test_update_emission_data_rounds_down() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000005,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply = 100_0001111;
        let supply_scalar = 1_0000000;
        e.as_contract(&pool, || {
            let reserve_emission_config = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0100000,
            };
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, res_token_index, supply, supply_scalar);
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
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
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);
        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply_scalar = 1_0000000;
        let user_balance = 0;
        e.as_contract(&pool, || {
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = 1 * 2 + res_token_type;
            update_user_emissions(
                &e,
                &reserve_emission_data,
                res_token_index,
                supply_scalar,
                &samwise,
                user_balance,
                false,
            );

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0);
        });
    }

    #[test]
    fn test_update_user_emissions_first_time_had_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);
        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply_scalar = 1_0000000;
        let user_balance = 0_5000000;
        e.as_contract(&pool, || {
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = 1 * 2 + res_token_type;
            update_user_emissions(
                &e,
                &reserve_emission_data,
                res_token_index,
                supply_scalar,
                &samwise,
                user_balance,
                false,
            );

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 6_1728394);
        });
    }

    #[test]
    fn test_update_user_emissions_no_bal_no_accrual() {
        let e = Env::default();
        e.mock_all_auths();
        let pool = testutils::create_pool(&e);

        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply_scalar = 1_0000000;
        let user_balance = 0;
        e.as_contract(&pool, || {
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 56789,
                accrued: 0_1000000,
            };

            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            update_user_emissions(
                &e,
                &reserve_emission_data,
                res_token_index,
                supply_scalar,
                &samwise,
                user_balance,
                false,
            );

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0_1000000);
        });
    }

    #[test]
    fn test_update_user_emissions_if_accrued_skips() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply_scalar = 1_0000000;
        let user_balance = 0_5000000;
        e.as_contract(&pool, || {
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 123456789,
                accrued: 1_1000000,
            };

            let res_token_type = 0;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            update_user_emissions(
                &e,
                &reserve_emission_data,
                res_token_index,
                supply_scalar,
                &samwise,
                user_balance,
                false,
            );

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, user_emission_data.accrued);
        });
    }

    #[test]
    fn test_update_user_emissions_accrues() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);
        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply_scalar = 1_0000000;
        let user_balance = 0_5000000;
        e.as_contract(&pool, || {
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 56789,
                accrued: 0_1000000,
            };

            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            update_user_emissions(
                &e,
                &reserve_emission_data,
                res_token_index,
                supply_scalar,
                &samwise,
                user_balance,
                false,
            );

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 6_2700000);
        });
    }

    #[test]
    fn test_update_user_emissions_claim_returns_accrual() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply_scalar = 1_0000000;
        let user_balance = 0_5000000;
        e.as_contract(&pool, || {
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 56789,
                accrued: 0_1000000,
            };

            let res_token_type = 1;
            let res_token_index = 1 * 2 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            let result = update_user_emissions(
                &e,
                &reserve_emission_data,
                res_token_index,
                supply_scalar,
                &samwise,
                user_balance,
                true,
            );

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0);
            assert_eq!(result, 6_2700000);
        });
    }

    #[test]
    fn test_update_user_emissions_claim_first_time_claims_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);

        let samwise = Address::generate(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let supply_scalar = 1_0000000;
        let user_balance = 0_5000000;
        e.as_contract(&pool, || {
            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = 1 * 2 + res_token_type;
            let result = update_user_emissions(
                &e,
                &reserve_emission_data,
                res_token_index,
                supply_scalar,
                &samwise,
                user_balance,
                true,
            );

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0);
            assert_eq!(result, 6_1728394);
        });
    }

    //********** execute claim **********/
    #[test]
    fn test_execute_claim() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();
        e.budget().reset_unlimited();

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let merry = Address::generate(&e);

        let (_, blnd_token_client) = testutils::create_blnd_token(&e, &pool, &bombadil);
        let (backstop, _) = testutils::create_backstop(&e);
        // mock backstop having emissions for pool
        e.as_contract(&backstop, || {
            blnd_token_client.approve(&backstop, &pool, &100_000_0000000_i128, &1000000);
        });
        blnd_token_client.mint(&backstop, &100_000_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_config.decimals = 5;
        reserve_data.b_supply = 100_00000;
        reserve_data.d_supply = 50_00000;
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_config.decimals = 9;
        reserve_config.index = 1;
        reserve_data.b_supply = 100_000_000_000;
        reserve_data.d_supply = 50_000_000_000;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let user_positions = Positions {
            liabilities: map![&e, (0, 2_00000)],
            collateral: map![&e, (1, 1_000_000_000)],
            supply: map![&e, (1, 1_000_000_000)],
        };
        e.as_contract(&pool, || {
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let reserve_emission_config_0 = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0100000,
            };
            let reserve_emission_data_0 = ReserveEmissionsData {
                index: 2345678,
                last_time: 1500000000,
            };
            let user_emission_data_0 = UserEmissionData {
                index: 1234567,
                accrued: 0_1000000,
            };
            let res_token_index_0 = 0 * 2 + 0; // d_token for reserve 0

            let reserve_emission_config_1 = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0150000,
            };
            let reserve_emission_data_1 = ReserveEmissionsData {
                index: 1345678,
                last_time: 1500000000,
            };
            let user_emission_data_1 = UserEmissionData {
                index: 1234567,
                accrued: 1_0000000,
            };
            let res_token_index_1 = 1 * 2 + 1; // b_token for reserve 1

            storage::set_res_emis_config(&e, &res_token_index_0, &reserve_emission_config_0);
            storage::set_res_emis_data(&e, &res_token_index_0, &reserve_emission_data_0);
            storage::set_user_emissions(&e, &samwise, &res_token_index_0, &user_emission_data_0);

            storage::set_res_emis_config(&e, &res_token_index_1, &reserve_emission_config_1);
            storage::set_res_emis_data(&e, &res_token_index_1, &reserve_emission_data_1);
            storage::set_user_emissions(&e, &samwise, &res_token_index_1, &user_emission_data_1);

            let reserve_token_ids: Vec<u32> = vec![&e, res_token_index_0, res_token_index_1];
            let result = execute_claim(&e, &samwise, &reserve_token_ids, &merry);

            let new_reserve_emission_data =
                storage::get_res_emis_data(&e, &res_token_index_0).unwrap_optimized();
            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index_0).unwrap_optimized();
            assert_eq!(new_reserve_emission_data.last_time, 1501000000);
            assert_eq!(
                new_user_emission_data.index,
                new_reserve_emission_data.index
            );
            assert_eq!(new_user_emission_data.accrued, 0);

            let new_reserve_emission_data_1 =
                storage::get_res_emis_data(&e, &res_token_index_1).unwrap_optimized();
            let new_user_emission_data_1 =
                storage::get_user_emissions(&e, &samwise, &res_token_index_1).unwrap_optimized();
            assert_eq!(new_reserve_emission_data_1.last_time, 1501000000);
            assert_eq!(
                new_user_emission_data_1.index,
                new_reserve_emission_data_1.index
            );
            assert_eq!(new_user_emission_data.accrued, 0);
            assert_eq!(result, 400_3222222 + 301_0222222);

            // verify tokens are sent
            assert_eq!(blnd_token_client.balance(&merry), 400_3222222 + 301_0222222);
            assert_eq!(
                blnd_token_client.balance(&backstop),
                100_000_0000000 - (400_3222222 + 301_0222222)
            )
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_calc_claim_with_invalid_reserve_panics() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();
        e.budget().reset_unlimited();

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let merry = Address::generate(&e);
        let (backstop, _) = testutils::create_backstop(&e);

        let (_, blnd_token_client) = testutils::create_blnd_token(&e, &pool, &bombadil);

        // mock backstop having emissions for pool
        e.as_contract(&backstop, || {
            blnd_token_client.approve(&backstop, &pool, &100_000_0000000_i128, &1000000);
        });
        blnd_token_client.mint(&backstop, &100_000_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 20,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_config.decimals = 5;
        reserve_data.b_supply = 100_00000;
        reserve_data.d_supply = 50_00000;
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_config.decimals = 9;
        reserve_config.index = 1;
        reserve_data.b_supply = 100_000_000_000;
        reserve_data.d_supply = 50_000_000_000;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let user_positions = Positions {
            liabilities: map![&e, (0, 2_00000)],
            collateral: map![&e, (1, 1_000_000_000)],
            supply: map![&e, (1, 1_000_000_000)],
        };
        e.as_contract(&pool, || {
            storage::set_backstop(&e, &backstop);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let reserve_emission_config_0 = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0100000,
            };
            let reserve_emission_data_0 = ReserveEmissionsData {
                index: 2345678,
                last_time: 1500000000,
            };
            let user_emission_data_0 = UserEmissionData {
                index: 1234567,
                accrued: 0_1000000,
            };
            let res_token_index_0 = 0 * 2 + 0; // d_token for reserve 0

            let reserve_emission_config_1 = ReserveEmissionsConfig {
                expiration: 1600000000,
                eps: 0_0150000,
            };
            let reserve_emission_data_1 = ReserveEmissionsData {
                index: 1345678,
                last_time: 1500000000,
            };
            let user_emission_data_1 = UserEmissionData {
                index: 1234567,
                accrued: 1_0000000,
            };
            let res_token_index_1 = 1 * 2 + 1; // b_token for reserve 1

            storage::set_res_emis_config(&e, &res_token_index_0, &reserve_emission_config_0);
            storage::set_res_emis_data(&e, &res_token_index_0, &reserve_emission_data_0);
            storage::set_user_emissions(&e, &samwise, &res_token_index_0, &user_emission_data_0);

            storage::set_res_emis_config(&e, &res_token_index_1, &reserve_emission_config_1);
            storage::set_res_emis_data(&e, &res_token_index_1, &reserve_emission_data_1);
            storage::set_user_emissions(&e, &samwise, &res_token_index_1, &user_emission_data_1);

            let reserve_token_ids: Vec<u32> = vec![&e, res_token_index_0, res_token_index_1, 6];
            execute_claim(&e, &samwise, &reserve_token_ids, &merry);

            assert_eq!(blnd_token_client.balance(&backstop), 100_000_0000000)
        });
    }
}
