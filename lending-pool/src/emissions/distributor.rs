use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{Address, Env, Vec};

use crate::{
    constants::SCALAR_7,
    dependencies::{BackstopClient, TokenClient},
    errors::PoolError,
    reserve::Reserve,
    storage::{self, ReserveEmissionsData, UserEmissionData},
};

/// Performs a claim against the given "reserve_token_ids" for "from"
pub fn execute_claim(
    e: &Env,
    from: &Address,
    reserve_token_ids: &Vec<u32>,
    to: &Address,
) -> Result<i128, PoolError> {
    let to_claim = calc_claim(&e, from, reserve_token_ids)?;

    if to_claim > 0 {
        let bkstp_addr = storage::get_backstop(e);
        let backstop = BackstopClient::new(&e, &bkstp_addr);
        backstop.claim(
            &e.current_contract_address(),
            &e.current_contract_id(),
            &to,
            &to_claim,
        );
    }

    Ok(to_claim)
}

/// Update the emissions information about a reserve token
///
/// ### Arguments
/// * `reserve` - The reserve being updated
/// * `res_token_type` - The reserve token being acted against (0 for dToken / 1 for bToken)
/// * `user` - The user performing an action against the reserve
///
/// ### Errors
/// If the reserve update failed
pub fn update_reserve(
    e: &Env,
    reserve: &Reserve,
    res_token_type: u32,
    user: &Address,
) -> Result<(), PoolError> {
    if let Some(res_emis_data) = update_emission_data(e, reserve, res_token_type)? {
        update_user_emissions(e, reserve, res_token_type, &res_emis_data, user, false)?;
        Ok(())
    } else {
        // no emissions data for the reserve exists - nothing to update
        Ok(())
    }
}

/// Determines emission total and resets all accrued emissions
///
/// Does not send tokens
///
/// ### Arguments
/// * `user` - The user to claim emissions for
/// * `reserve_token_ids` - Vector of reserve token ids
fn calc_claim(e: &Env, user: &Address, reserve_token_ids: &Vec<u32>) -> Result<i128, PoolError> {
    let reserve_list = storage::get_res_list(e);
    let mut to_claim = 0;
    for id in reserve_token_ids.clone() {
        // assumption is made that it is unlikely both reserve tokens will be claiming emissions in the same call
        // TODO: verify this, if not, optimize the duplicate reserve call
        let reserve_token_id = id.unwrap();
        let reserve_addr = reserve_list.get(reserve_token_id / 3);
        match reserve_addr {
            Some(res_addr) => {
                let reserve = Reserve::load(&e, res_addr.unwrap());
                let to_claim_from_reserve =
                    update_and_claim(&e, &reserve, reserve_token_id % 3, user).unwrap();
                to_claim += to_claim_from_reserve;
            }
            None => {
                return Err(PoolError::BadRequest);
            }
        }
    }

    Ok(to_claim)
}

/// Update and claim emissions for a user
///
/// ### Arguments
/// * `reserve` - The reserve being claimed
/// * `res_token_type` - The reserve token being claimed (0 for dToken / 1 for bToken)
/// * `user` - The user claiming
fn update_and_claim(
    e: &Env,
    reserve: &Reserve,
    res_token_type: u32,
    user: &Address,
) -> Result<i128, PoolError> {
    if let Some(res_emis_data) = update_emission_data(e, reserve, res_token_type)? {
        update_user_emissions(e, reserve, res_token_type, &res_emis_data, user, true)
    } else {
        // no emissions data for the reserve exists
        // TODO: consider throwing error
        Ok(0)
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
    let res_token_index: u32 = reserve.config.index * 3 + res_token_type;
    let token_emission_config = match storage::get_res_emis_config(e, &res_token_index) {
        Some(res) => res,
        None => return Ok(None), // no emission exist, no update is required
    };
    let token_emission_data = storage::get_res_emis_data(e, &res_token_index).unwrap(); // exists if config is written to

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

    let additional_idx = (i128(ledger_timestamp - token_emission_data.last_time)
        * i128(token_emission_config.eps))
    .fixed_div_floor(total_supply, SCALAR_7)
    .unwrap();
    let new_data = ReserveEmissionsData {
        index: additional_idx + token_emission_data.index,
        last_time: ledger_timestamp,
    };
    storage::set_res_emis_data(e, &res_token_index, &new_data);
    Ok(Some(new_data))
}

pub fn update_user_emissions(
    e: &Env,
    reserve: &Reserve,
    res_token_type: u32,
    res_emis_data: &ReserveEmissionsData,
    user: &Address,
    to_claim: bool,
) -> Result<i128, PoolError> {
    let res_token_index: u32 = (reserve.config.index * 3) + res_token_type;

    let token_addr = if res_token_type == 0 {
        &reserve.config.d_token
    } else {
        &reserve.config.b_token
    };
    let user_bal = TokenClient::new(&e, token_addr).balance(&user);

    if let Some(user_data) = storage::get_user_emissions(e, &user, &res_token_index) {
        if user_data.index != res_emis_data.index || to_claim {
            let mut accrual = user_data.accrued;
            if user_bal != 0 {
                let to_accrue = user_bal
                    .fixed_mul_floor(res_emis_data.index - user_data.index, SCALAR_7)
                    .unwrap();
                accrual += to_accrue;
            }
            return Ok(set_user_emissions(
                e,
                &user,
                res_token_index,
                res_emis_data.index,
                accrual,
                to_claim,
            ));
        }
        return Ok(0);
    } else if user_bal == 0 {
        // first time the user registered an action with the asset since emissions were added
        return Ok(set_user_emissions(
            e,
            &user,
            res_token_index,
            res_emis_data.index,
            0,
            to_claim,
        ));
    } else {
        // user had tokens before emissions began, they are due any historical emissions
        let to_accrue = user_bal
            .fixed_mul_floor(res_emis_data.index, SCALAR_7)
            .unwrap();
        return Ok(set_user_emissions(
            e,
            &user,
            res_token_index,
            res_emis_data.index,
            to_accrue,
            to_claim,
        ));
    }
}

fn set_user_emissions(
    e: &Env,
    user: &Address,
    res_token_index: u32,
    index: i128,
    accrued: i128,
    to_claim: bool,
) -> i128 {
    if to_claim {
        storage::set_user_emissions(
            e,
            &user,
            &res_token_index,
            &UserEmissionData { index, accrued: 0 },
        );
        return accrued;
    } else {
        storage::set_user_emissions(
            e,
            &user,
            &res_token_index,
            &UserEmissionData { index, accrued },
        );
        return 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{ReserveConfig, ReserveData, ReserveEmissionsConfig},
        testutils::{create_token_contract, generate_contract_id},
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
        vec, BytesN,
    };

    /********** update_reserve **********/

    #[test]
    fn test_update_happy_path() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);
        res_token_client.mint(&bombadil, &samwise, &2_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });
        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id,
                100_0000000,
                50_0000000,
            );

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

            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            let _result = update_reserve(&e, &reserve, res_token_type, &samwise);

            let new_reserve_emission_data =
                storage::get_res_emis_data(&e, &res_token_index).unwrap();
            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
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
        let pool_id = generate_contract_id(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);
        let (res_token_id, _) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id,
                100_0000000,
                50_0000000,
            );

            let res_token_type = 1;
            let res_token_index = reserve.config.index * 3 + res_token_type;

            let result = update_reserve(&e, &reserve, res_token_type, &samwise);
            match result {
                Ok(_) => {
                    assert!(storage::get_res_emis_data(&e, &res_token_index).is_none());
                    assert!(storage::get_user_emissions(&e, &samwise, &res_token_index).is_none());
                }
                Err(_) => assert!(false),
            }
        });
    }

    /********** calc_claim **********/

    #[test]
    fn test_calc_claim_happy_path() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id_0, res_token_client_0) = create_token_contract(&e, &bombadil);
        let (res_token_id_1, res_token_client_1) = create_token_contract(&e, &bombadil);
        res_token_client_0.mint(&bombadil, &samwise, &2_0000000);
        res_token_client_1.mint(&bombadil, &samwise, &2_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve_0 = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id_0,
                100_0000000,
                50_0000000,
            );
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
            let res_token_index_0 = reserve_0.config.index * 3 + 0; // d_token for reserve 0

            let reserve_1 = setup_reserve(
                &e,
                res_token_id_1,
                generate_contract_id(&e),
                100_0000000,
                50_0000000,
            );
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
            let res_token_index_1 = reserve_1.config.index * 3 + 1; // b_token for reserve 1

            storage::set_res_emis_config(&e, &res_token_index_0, &reserve_emission_config_0);
            storage::set_res_emis_data(&e, &res_token_index_0, &reserve_emission_data_0);
            storage::set_user_emissions(&e, &samwise, &res_token_index_0, &user_emission_data_0);

            storage::set_res_emis_config(&e, &res_token_index_1, &reserve_emission_config_1);
            storage::set_res_emis_data(&e, &res_token_index_1, &reserve_emission_data_1);
            storage::set_user_emissions(&e, &samwise, &res_token_index_1, &user_emission_data_1);

            let reserve_token_ids: Vec<u32> = vec![&e, res_token_index_0, res_token_index_1];
            let result = calc_claim(&e, &samwise, &reserve_token_ids);

            let new_reserve_emission_data =
                storage::get_res_emis_data(&e, &res_token_index_0).unwrap();
            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index_0).unwrap();
            assert_eq!(new_reserve_emission_data.last_time, 1501000000);
            assert_eq!(
                new_user_emission_data.index,
                new_reserve_emission_data.index
            );
            assert_eq!(new_user_emission_data.accrued, 0);

            let new_reserve_emission_data_1 =
                storage::get_res_emis_data(&e, &res_token_index_1).unwrap();
            let new_user_emission_data_1 =
                storage::get_user_emissions(&e, &samwise, &res_token_index_1).unwrap();
            assert_eq!(new_reserve_emission_data_1.last_time, 1501000000);
            assert_eq!(
                new_user_emission_data_1.index,
                new_reserve_emission_data_1.index
            );
            assert_eq!(new_user_emission_data.accrued, 0);

            assert_eq!(result.unwrap(), 400_3222222 + 301_0222222);
        });
    }

    #[test]
    fn test_calc_claim_with_invalid_reserve_panics() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id_0, res_token_client_0) = create_token_contract(&e, &bombadil);
        let (res_token_id_1, res_token_client_1) = create_token_contract(&e, &bombadil);
        res_token_client_0.mint(&bombadil, &samwise, &2_0000000);
        res_token_client_1.mint(&bombadil, &samwise, &2_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve_0 = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id_0,
                100_0000000,
                50_0000000,
            );
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
            let res_token_index_0 = reserve_0.config.index * 3 + 0; // d_token for reserve 0

            let reserve_1 = setup_reserve(
                &e,
                res_token_id_1,
                generate_contract_id(&e),
                100_0000000,
                50_0000000,
            );
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
            let res_token_index_1 = reserve_1.config.index * 3 + 1; // b_token for reserve 1

            storage::set_res_emis_config(&e, &res_token_index_0, &reserve_emission_config_0);
            storage::set_res_emis_data(&e, &res_token_index_0, &reserve_emission_data_0);
            storage::set_user_emissions(&e, &samwise, &res_token_index_0, &user_emission_data_0);

            storage::set_res_emis_config(&e, &res_token_index_1, &reserve_emission_config_1);
            storage::set_res_emis_data(&e, &res_token_index_1, &reserve_emission_data_1);
            storage::set_user_emissions(&e, &samwise, &res_token_index_1, &user_emission_data_1);

            let reserve_token_ids: Vec<u32> = vec![&e, res_token_index_0, res_token_index_1, 6]; // d_token of res 3 added
            let result = calc_claim(&e, &samwise, &reserve_token_ids);
            match result {
                Ok(_) => {
                    assert!(false)
                }
                Err(err) => {
                    assert_eq!(err, PoolError::BadRequest);
                }
            }
        });
    }

    /********** update_and_claim **********/

    #[test]
    fn test_update_and_claim_happy_path() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);
        res_token_client.mint(&bombadil, &samwise, &2_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id,
                100_0000000,
                50_0000000,
            );

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

            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            let result = update_and_claim(&e, &reserve, res_token_type, &samwise);

            let new_reserve_emission_data =
                storage::get_res_emis_data(&e, &res_token_index).unwrap();
            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_reserve_emission_data.last_time, 1501000000);
            assert_eq!(
                new_user_emission_data.index,
                new_reserve_emission_data.index
            );
            assert_eq!(new_user_emission_data.accrued, 0);
            assert_eq!(result.unwrap(), 400_3222222);
        });
    }

    /********** update emission data **********/

    #[test]
    fn test_update_emission_data_no_config_ignores() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000, // 10^6 seconds have passed
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                generate_contract_id(&e),
                100_0000000,
                50_0000000,
            );

            let res_token_type = 1;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            // no emission information stored

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
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

        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                generate_contract_id(&e),
                100_0000000,
                50_0000000,
            );

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
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap();
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

        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                generate_contract_id(&e),
                100_0000000,
                50_0000000,
            );

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
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap();
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

        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                generate_contract_id(&e),
                100_0000000,
                50_0000000,
            );

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
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap();
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

        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1501000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                generate_contract_id(&e),
                0,
                100_0000000,
            );

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
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap();
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

        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1700000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                generate_contract_id(&e),
                0,
                100_0000000,
            );

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
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap();
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

        let pool_id = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000005,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                generate_contract_id(&e),
                100_0001111,
                0,
            );

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
            storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
            storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

            let result = update_emission_data(&e, &reserve, res_token_type).unwrap();
            match result {
                Some(_) => {
                    let new_reserve_emission_data =
                        storage::get_res_emis_data(&e, &res_token_index).unwrap();
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

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, _res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id,
                100_0000000,
                50_0000000,
            );

            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                &samwise,
                false,
            )
            .unwrap();

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0);
        });
    }

    #[test]
    fn test_update_user_emissions_first_time_had_tokens() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);
        res_token_client.mint(&bombadil, &samwise, &0_5000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id,
                100_0000000,
                50_0000000,
            );

            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                &samwise,
                false,
            )
            .unwrap();

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 6_1728394);
        });
    }

    #[test]
    fn test_update_user_emissions_no_bal_no_accrual() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);
        let (res_token_id, _res_token_client) = create_token_contract(&e, &bombadil);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                res_token_id,
                generate_contract_id(&e),
                60_0000000,
                50_0000000,
            );

            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 56789,
                accrued: 0_1000000,
            };

            let res_token_type = 1;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                &samwise,
                false,
            )
            .unwrap();

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0_1000000);
        });
    }

    #[test]
    fn test_update_user_emissions_if_accrued_skips() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);
        res_token_client.mint(&bombadil, &samwise, &0_5000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id,
                60_0000000,
                50_0000000,
            );

            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 123456789,
                accrued: 1_1000000,
            };

            let res_token_type = 0;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                &samwise,
                false,
            )
            .unwrap();

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, user_emission_data.accrued);
        });
    }

    #[test]
    fn test_update_user_emissions_accrues() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);
        res_token_client.mint(&bombadil, &samwise, &0_5000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                res_token_id,
                generate_contract_id(&e),
                60_0000000,
                50_0000000,
            );

            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 56789,
                accrued: 0_1000000,
            };

            let res_token_type = 1;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                &samwise,
                false,
            )
            .unwrap();

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 6_2700000);
        });
    }

    #[test]
    fn test_update_user_emissions_claim_returns_accrual() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);
        res_token_client.mint(&bombadil, &samwise, &0_5000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                res_token_id,
                generate_contract_id(&e),
                60_0000000,
                50_0000000,
            );

            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };
            let user_emission_data = UserEmissionData {
                index: 56789,
                accrued: 0_1000000,
            };

            let res_token_type = 1;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

            let result = update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                &samwise,
                true,
            )
            .unwrap();

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0);
            assert_eq!(result, 6_2700000);
        });
    }

    #[test]
    fn test_update_user_emissions_claim_first_time_claims_tokens() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let (res_token_id, res_token_client) = create_token_contract(&e, &bombadil);
        res_token_client.mint(&bombadil, &samwise, &0_5000000);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let reserve = setup_reserve(
                &e,
                generate_contract_id(&e),
                res_token_id,
                100_0000000,
                50_0000000,
            );

            let reserve_emission_data = ReserveEmissionsData {
                index: 123456789,
                last_time: 1500000000,
            };

            let res_token_type = 0;
            let res_token_index = reserve.config.index * 3 + res_token_type;
            let result = update_user_emissions(
                &e,
                &reserve,
                res_token_type,
                &reserve_emission_data,
                &samwise,
                true,
            )
            .unwrap();

            let new_user_emission_data =
                storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap();
            assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
            assert_eq!(new_user_emission_data.accrued, 0);
            assert_eq!(result, 6_1728394);
        });
    }

    /********** Test Helpers **********/

    fn setup_reserve(
        e: &Env,
        b_token_id: BytesN<32>,
        d_token_id: BytesN<32>,
        b_supply: i128,
        d_supply: i128,
    ) -> Reserve {
        let res_addr = generate_contract_id(&e);
        let index = storage::get_res_list(e).len();
        let res_config = ReserveConfig {
            b_token: b_token_id,
            d_token: d_token_id,
            decimals: 7,
            c_factor: 0,
            l_factor: 0,
            util: 0_7500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_010_000,
            index,
        };
        let res_data = ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 1_000_000_000,
            b_supply,
            d_supply,
            last_block: 123,
        };
        storage::set_res_config(e, &res_addr, &res_config);
        storage::set_res_data(e, &res_addr, &res_data);
        Reserve {
            asset: generate_contract_id(&e),
            config: res_config,
            data: res_data,
        }
    }
}
