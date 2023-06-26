use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, Address, Env, Vec};

use crate::{
    dependencies::BackstopClient,
    errors::PoolError,
    storage::{self, ReserveEmissionsData, UserEmissionData},
};

/// Performs a claim against the given "reserve_token_ids" for "from"
pub fn execute_claim(e: &Env, from: &Address, reserve_token_ids: &Vec<u32>, to: &Address) -> i128 {
    let positions = storage::get_user_positions(e, &from);
    let reserve_list = storage::get_res_list(e);
    let mut to_claim = 0;
    for id in reserve_token_ids.clone() {
        let reserve_token_id = id.unwrap_optimized();
        let reserve_index = reserve_token_id / 2;
        let reserve_addr = reserve_list.get(reserve_index);
        match reserve_addr {
            Some(res_addr) => {
                let res_address = res_addr.unwrap_optimized();
                let reserve_config = storage::get_res_config(e, &res_address);
                let reserve_data = storage::get_res_data(e, &res_address);
                let (user_balance, supply) = match reserve_token_id % 2 {
                    0 => (
                        positions.get_liabilities(reserve_index),
                        reserve_data.d_supply,
                    ),
                    1 => (
                        positions.get_collateral(reserve_index),
                        reserve_data.b_supply,
                    ),
                    _ => panic_with_error!(e, PoolError::BadRequest),
                };
                to_claim += update_emissions(
                    &e,
                    reserve_token_id,
                    supply,
                    10i128.pow(reserve_config.decimals),
                    &from,
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
        let bkstp_addr = storage::get_backstop(e);
        let backstop = BackstopClient::new(&e, &bkstp_addr);
        backstop.pool_claim(&e.current_contract_address(), &to, &to_claim);
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
    let token_emission_config = match storage::get_res_emis_config(e, &res_token_id) {
        Some(res) => res,
        None => return None, // no emission exist, no update is required
    };
    let token_emission_data = storage::get_res_emis_data(e, &res_token_id).unwrap_optimized(); // exists if config is written to

    if token_emission_data.last_time >= token_emission_config.expiration
        || e.ledger().timestamp() == token_emission_data.last_time
        || token_emission_config.eps == 0
        || supply == 0
    {
        return Some(token_emission_data);
    }

    let ledger_timestamp = if e.ledger().timestamp() > token_emission_config.expiration {
        token_emission_config.expiration
    } else {
        e.ledger().timestamp()
    };

    let additional_idx = (i128(ledger_timestamp - token_emission_data.last_time)
        * i128(token_emission_config.eps))
    .fixed_div_floor(supply, supply_scalar)
    .unwrap_optimized();
    let new_data = ReserveEmissionsData {
        index: additional_idx + token_emission_data.index,
        last_time: e.ledger().timestamp(),
    };
    storage::set_res_emis_data(e, &res_token_id, &new_data);
    Some(new_data)
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
    if let Some(user_data) = storage::get_user_emissions(e, &user, &res_token_id) {
        if user_data.index != res_emis_data.index || claim {
            let mut accrual = user_data.accrued;
            if balance != 0 {
                let to_accrue = balance
                    .fixed_mul_floor(res_emis_data.index - user_data.index, supply_scalar)
                    .unwrap_optimized();
                accrual += to_accrue;
            }
            return set_user_emissions(e, &user, res_token_id, res_emis_data.index, accrual, claim);
        }
        return 0;
    } else if balance == 0 {
        // first time the user registered an action with the asset since emissions were added
        return set_user_emissions(e, &user, res_token_id, res_emis_data.index, 0, claim);
    } else {
        // user had tokens before emissions began, they are due any historical emissions
        let to_accrue = balance
            .fixed_mul_floor(res_emis_data.index, supply_scalar)
            .unwrap_optimized();
        return set_user_emissions(
            e,
            &user,
            res_token_id,
            res_emis_data.index,
            to_accrue,
            claim,
        );
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
            &user,
            &res_token_id,
            &UserEmissionData { index, accrued: 0 },
        );
        return accrued;
    } else {
        storage::set_user_emissions(
            e,
            &user,
            &res_token_id,
            &UserEmissionData { index, accrued },
        );
        return 0;
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         storage::ReserveEmissionsConfig,
//         testutils::{create_reserve, setup_reserve},
//     };

//     use super::*;
//     use soroban_sdk::{
//         testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
//         vec,
//     };

//     /********** update_reserve **********/
//     #[test]
//     fn test_update_happy_path() {
//         let e = Env::default();
//         e.mock_all_auths();
//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000, // 10^6 seconds have passed
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         let res_token_client = TokenClient::new(&e, &reserve.config.d_token);
//         res_token_client.mint(&samwise, &2_0000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data = UserEmissionData {
//                 index: 1234567,
//                 accrued: 0_1000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;

//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);
//             storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

//             let _result = update_reserve(&e, &reserve, res_token_type, &samwise);

//             let new_reserve_emission_data =
//                 storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_reserve_emission_data.last_time, 1501000000);
//             assert_eq!(
//                 new_user_emission_data.index,
//                 new_reserve_emission_data.index
//             );
//             assert_eq!(new_user_emission_data.accrued, 400_3222222);
//         });
//     }

//     #[test]
//     fn test_update_no_config_ignores() {
//         let e = Env::default();
//         e.mock_all_auths();
//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000, // 10^6 seconds have passed
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;

//             let result = update_reserve(&e, &reserve, res_token_type, &samwise);
//             match result {
//                 Ok(_) => {
//                     assert!(storage::get_res_emis_data(&e, &res_token_index).is_none());
//                     assert!(storage::get_user_emissions(&e, &samwise, &res_token_index).is_none());
//                 }
//                 Err(_) => assert!(false),
//             }
//         });
//     }

//     /********** calc_claim **********/
//     #[test]
//     fn test_calc_claim_happy_path() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000, // 10^6 seconds have passed
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.data.b_supply = 100_0000000;
//         reserve_0.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.config.index = 1;
//         reserve_1.data.b_supply = 100_0000000;
//         reserve_1.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

//         let res_token_client_0 = TokenClient::new(&e, &reserve_0.config.d_token);
//         res_token_client_0.mint(&samwise, &2_0000000);

//         let res_token_client_1 = TokenClient::new(&e, &reserve_1.config.b_token);
//         res_token_client_1.mint(&samwise, &2_0000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config_0 = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data_0 = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data_0 = UserEmissionData {
//                 index: 1234567,
//                 accrued: 0_1000000,
//             };
//             let res_token_index_0 = reserve_0.config.index * 3 + 0; // d_token for reserve 0

//             let reserve_emission_config_1 = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0150000,
//             };
//             let reserve_emission_data_1 = ReserveEmissionsData {
//                 index: 1345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data_1 = UserEmissionData {
//                 index: 1234567,
//                 accrued: 1_0000000,
//             };
//             let res_token_index_1 = reserve_1.config.index * 3 + 1; // b_token for reserve 1

//             storage::set_res_emis_config(&e, &res_token_index_0, &reserve_emission_config_0);
//             storage::set_res_emis_data(&e, &res_token_index_0, &reserve_emission_data_0);
//             storage::set_user_emissions(&e, &samwise, &res_token_index_0, &user_emission_data_0);

//             storage::set_res_emis_config(&e, &res_token_index_1, &reserve_emission_config_1);
//             storage::set_res_emis_data(&e, &res_token_index_1, &reserve_emission_data_1);
//             storage::set_user_emissions(&e, &samwise, &res_token_index_1, &user_emission_data_1);

//             let reserve_token_ids: Vec<u32> = vec![&e, res_token_index_0, res_token_index_1];
//             let result = calc_claim(&e, &samwise, &reserve_token_ids);

//             let new_reserve_emission_data =
//                 storage::get_res_emis_data(&e, &res_token_index_0).unwrap_optimized();
//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index_0).unwrap_optimized();
//             assert_eq!(new_reserve_emission_data.last_time, 1501000000);
//             assert_eq!(
//                 new_user_emission_data.index,
//                 new_reserve_emission_data.index
//             );
//             assert_eq!(new_user_emission_data.accrued, 0);

//             let new_reserve_emission_data_1 =
//                 storage::get_res_emis_data(&e, &res_token_index_1).unwrap_optimized();
//             let new_user_emission_data_1 =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index_1).unwrap_optimized();
//             assert_eq!(new_reserve_emission_data_1.last_time, 1501000000);
//             assert_eq!(
//                 new_user_emission_data_1.index,
//                 new_reserve_emission_data_1.index
//             );
//             assert_eq!(new_user_emission_data.accrued, 0);

//             assert_eq!(result.unwrap_optimized(), 400_3222222 + 301_0222222);
//         });
//     }

//     #[test]
//     fn test_calc_claim_alternative_decimals() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000, // 10^6 seconds have passed
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.config.decimals = 5;
//         reserve_0.scalar = 1_00000;
//         reserve_0.data.b_supply = 100_00000;
//         reserve_0.data.d_supply = 50_00000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.config.decimals = 9;
//         reserve_1.scalar = 1_000_000_000;
//         reserve_1.config.index = 1;
//         reserve_1.data.b_supply = 100_000_000_000;
//         reserve_1.data.d_supply = 50_000_000_000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

//         let res_token_client_0 = TokenClient::new(&e, &reserve_0.config.d_token);
//         res_token_client_0.mint(&samwise, &2_00000);

//         let res_token_client_1 = TokenClient::new(&e, &reserve_1.config.b_token);
//         res_token_client_1.mint(&samwise, &2_000_000_000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config_0 = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data_0 = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data_0 = UserEmissionData {
//                 index: 1234567,
//                 accrued: 0_1000000,
//             };
//             let res_token_index_0 = reserve_0.config.index * 3 + 0; // d_token for reserve 0

//             let reserve_emission_config_1 = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0150000,
//             };
//             let reserve_emission_data_1 = ReserveEmissionsData {
//                 index: 1345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data_1 = UserEmissionData {
//                 index: 1234567,
//                 accrued: 1_0000000,
//             };
//             let res_token_index_1 = reserve_1.config.index * 3 + 1; // b_token for reserve 1

//             storage::set_res_emis_config(&e, &res_token_index_0, &reserve_emission_config_0);
//             storage::set_res_emis_data(&e, &res_token_index_0, &reserve_emission_data_0);
//             storage::set_user_emissions(&e, &samwise, &res_token_index_0, &user_emission_data_0);

//             storage::set_res_emis_config(&e, &res_token_index_1, &reserve_emission_config_1);
//             storage::set_res_emis_data(&e, &res_token_index_1, &reserve_emission_data_1);
//             storage::set_user_emissions(&e, &samwise, &res_token_index_1, &user_emission_data_1);

//             let reserve_token_ids: Vec<u32> = vec![&e, res_token_index_0, res_token_index_1];
//             let result = calc_claim(&e, &samwise, &reserve_token_ids);

//             let new_reserve_emission_data =
//                 storage::get_res_emis_data(&e, &res_token_index_0).unwrap_optimized();
//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index_0).unwrap_optimized();
//             assert_eq!(new_reserve_emission_data.last_time, 1501000000);
//             assert_eq!(
//                 new_user_emission_data.index,
//                 new_reserve_emission_data.index
//             );
//             assert_eq!(new_user_emission_data.accrued, 0);

//             let new_reserve_emission_data_1 =
//                 storage::get_res_emis_data(&e, &res_token_index_1).unwrap_optimized();
//             let new_user_emission_data_1 =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index_1).unwrap_optimized();
//             assert_eq!(new_reserve_emission_data_1.last_time, 1501000000);
//             assert_eq!(
//                 new_user_emission_data_1.index,
//                 new_reserve_emission_data_1.index
//             );
//             assert_eq!(new_user_emission_data.accrued, 0);

//             assert_eq!(result.unwrap_optimized(), 400_3222222 + 301_0222222);
//         });
//     }

//     #[test]
//     fn test_calc_claim_with_invalid_reserve_panics() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000, // 10^6 seconds have passed
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve_0 = create_reserve(&e);
//         reserve_0.data.b_supply = 100_0000000;
//         reserve_0.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

//         let mut reserve_1 = create_reserve(&e);
//         reserve_1.config.index = 1;
//         reserve_1.data.b_supply = 100_0000000;
//         reserve_1.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

//         let res_token_client_0 = TokenClient::new(&e, &reserve_0.config.d_token);
//         res_token_client_0.mint(&samwise, &2_0000000);

//         let res_token_client_1 = TokenClient::new(&e, &reserve_1.config.b_token);
//         res_token_client_1.mint(&samwise, &2_0000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config_0 = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data_0 = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data_0 = UserEmissionData {
//                 index: 1234567,
//                 accrued: 0_1000000,
//             };
//             let res_token_index_0 = reserve_0.config.index * 3 + 0; // d_token for reserve 0

//             let reserve_emission_config_1 = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0150000,
//             };
//             let reserve_emission_data_1 = ReserveEmissionsData {
//                 index: 1345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data_1 = UserEmissionData {
//                 index: 1234567,
//                 accrued: 1_0000000,
//             };
//             let res_token_index_1 = reserve_1.config.index * 3 + 1; // b_token for reserve 1

//             storage::set_res_emis_config(&e, &res_token_index_0, &reserve_emission_config_0);
//             storage::set_res_emis_data(&e, &res_token_index_0, &reserve_emission_data_0);
//             storage::set_user_emissions(&e, &samwise, &res_token_index_0, &user_emission_data_0);

//             storage::set_res_emis_config(&e, &res_token_index_1, &reserve_emission_config_1);
//             storage::set_res_emis_data(&e, &res_token_index_1, &reserve_emission_data_1);
//             storage::set_user_emissions(&e, &samwise, &res_token_index_1, &user_emission_data_1);

//             let reserve_token_ids: Vec<u32> = vec![&e, res_token_index_0, res_token_index_1, 6]; // d_token of res 3 added
//             let result = calc_claim(&e, &samwise, &reserve_token_ids);
//             match result {
//                 Ok(_) => {
//                     assert!(false)
//                 }
//                 Err(err) => {
//                     assert_eq!(err, PoolError::BadRequest);
//                 }
//             }
//         });
//     }

//     /********** update_and_claim **********/
//     #[test]
//     fn test_update_and_claim_happy_path() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000, // 10^6 seconds have passed
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         let res_token_client = TokenClient::new(&e, &reserve.config.d_token);
//         res_token_client.mint(&samwise, &2_0000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1500000000,
//             };
//             let user_emission_data = UserEmissionData {
//                 index: 1234567,
//                 accrued: 0_1000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;

//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);
//             storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

//             let result = update_and_claim(&e, &reserve, res_token_type, &samwise);

//             let new_reserve_emission_data =
//                 storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_reserve_emission_data.last_time, 1501000000);
//             assert_eq!(
//                 new_user_emission_data.index,
//                 new_reserve_emission_data.index
//             );
//             assert_eq!(new_user_emission_data.accrued, 0);
//             assert_eq!(result.unwrap_optimized(), 400_3222222);
//         });
//     }

//     /********** update emission data **********/
//     #[test]
//     fn test_update_emission_data_no_config_ignores() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000, // 10^6 seconds have passed
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             // no emission information stored

//             let result = update_emission_data(&e, &reserve, res_token_type).unwrap_optimized();
//             match result {
//                 Some(_) => {
//                     assert!(false)
//                 }
//                 None => {
//                     assert!(storage::get_res_emis_data(&e, &res_token_index).is_none());
//                     assert!(storage::get_res_emis_config(&e, &res_token_index).is_none());
//                 }
//             }
//         });
//     }

//     #[test]
//     fn test_update_emission_data_expired_returns_old() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1600000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

//             let result = update_emission_data(&e, &reserve, res_token_type).unwrap_optimized();
//             match result {
//                 Some(_) => {
//                     let new_reserve_emission_data =
//                         storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//                     assert_eq!(
//                         new_reserve_emission_data.last_time,
//                         reserve_emission_data.last_time
//                     );
//                     assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
//                 }
//                 None => assert!(false),
//             }
//         });
//     }

//     #[test]
//     fn test_update_emission_data_updated_this_block_returns_old() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1501000000,
//             };

//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

//             let result = update_emission_data(&e, &reserve, res_token_type).unwrap_optimized();
//             match result {
//                 Some(_) => {
//                     let new_reserve_emission_data =
//                         storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//                     assert_eq!(
//                         new_reserve_emission_data.last_time,
//                         reserve_emission_data.last_time
//                     );
//                     assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
//                 }
//                 None => assert!(false),
//             }
//         });
//     }

//     #[test]
//     fn test_update_emission_data_no_eps_returns_old() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1500000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

//             let result = update_emission_data(&e, &reserve, res_token_type).unwrap_optimized();
//             match result {
//                 Some(_) => {
//                     let new_reserve_emission_data =
//                         storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//                     assert_eq!(
//                         new_reserve_emission_data.last_time,
//                         reserve_emission_data.last_time
//                     );
//                     assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
//                 }
//                 None => assert!(false),
//             }
//         });
//     }

//     #[test]
//     fn test_update_emission_data_no_supply_returns_old() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1501000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 0;
//         reserve.data.d_supply = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 2345678,
//                 last_time: 1500000000,
//             };

//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

//             let result = update_emission_data(&e, &reserve, res_token_type).unwrap_optimized();
//             match result {
//                 Some(_) => {
//                     let new_reserve_emission_data =
//                         storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//                     assert_eq!(
//                         new_reserve_emission_data.last_time,
//                         reserve_emission_data.last_time
//                     );
//                     assert_eq!(new_reserve_emission_data.index, reserve_emission_data.index);
//                 }
//                 None => assert!(false),
//             }
//         });
//     }

//     #[test]
//     fn test_update_emission_data_d_token_past_exp() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1700000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 200_0000000;
//         reserve.data.d_supply = 100_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000001,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

//             let result = update_emission_data(&e, &reserve, res_token_type).unwrap_optimized();
//             match result {
//                 Some(_) => {
//                     let new_reserve_emission_data =
//                         storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//                     assert_eq!(new_reserve_emission_data.last_time, 1700000000);
//                     assert_eq!(new_reserve_emission_data.index, 10012_3457789);
//                 }
//                 None => assert!(false),
//             }
//         });
//     }

//     #[test]
//     fn test_update_emission_data_b_token_rounds_down() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000005,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0001111;
//         reserve.data.d_supply = 0;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_config = ReserveEmissionsConfig {
//                 expiration: 1600000000,
//                 eps: 0_0100000,
//             };
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };

//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_res_emis_config(&e, &res_token_index, &reserve_emission_config);
//             storage::set_res_emis_data(&e, &res_token_index, &reserve_emission_data);

//             let result = update_emission_data(&e, &reserve, res_token_type).unwrap_optimized();
//             match result {
//                 Some(_) => {
//                     let new_reserve_emission_data =
//                         storage::get_res_emis_data(&e, &res_token_index).unwrap_optimized();
//                     assert_eq!(new_reserve_emission_data.last_time, 1500000005);
//                     assert_eq!(new_reserve_emission_data.index, 123461788);
//                 }
//                 None => assert!(false),
//             }
//         });
//     }

//     /********** update_user_emissions **********/
//     #[test]
//     fn test_update_user_emissions_first_time() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);
//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             update_user_emissions(
//                 &e,
//                 &reserve,
//                 res_token_type,
//                 &reserve_emission_data,
//                 &samwise,
//                 false,
//             )
//             .unwrap_optimized();

//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
//             assert_eq!(new_user_emission_data.accrued, 0);
//         });
//     }

//     #[test]
//     fn test_update_user_emissions_first_time_had_tokens() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         let res_token_client = TokenClient::new(&e, &reserve.config.d_token);
//         res_token_client.mint(&samwise, &0_5000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             update_user_emissions(
//                 &e,
//                 &reserve,
//                 res_token_type,
//                 &reserve_emission_data,
//                 &samwise,
//                 false,
//             )
//             .unwrap_optimized();

//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
//             assert_eq!(new_user_emission_data.accrued, 6_1728394);
//         });
//     }

//     #[test]
//     fn test_update_user_emissions_no_bal_no_accrual() {
//         let e = Env::default();
//         e.mock_all_auths();
//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 60_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };
//             let user_emission_data = UserEmissionData {
//                 index: 56789,
//                 accrued: 0_1000000,
//             };

//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

//             update_user_emissions(
//                 &e,
//                 &reserve,
//                 res_token_type,
//                 &reserve_emission_data,
//                 &samwise,
//                 false,
//             )
//             .unwrap_optimized();

//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
//             assert_eq!(new_user_emission_data.accrued, 0_1000000);
//         });
//     }

//     #[test]
//     fn test_update_user_emissions_if_accrued_skips() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         let res_token_client = TokenClient::new(&e, &reserve.config.d_token);
//         res_token_client.mint(&samwise, &0_5000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };
//             let user_emission_data = UserEmissionData {
//                 index: 123456789,
//                 accrued: 1_1000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

//             update_user_emissions(
//                 &e,
//                 &reserve,
//                 res_token_type,
//                 &reserve_emission_data,
//                 &samwise,
//                 false,
//             )
//             .unwrap_optimized();

//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
//             assert_eq!(new_user_emission_data.accrued, user_emission_data.accrued);
//         });
//     }

//     #[test]
//     fn test_update_user_emissions_accrues() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 60_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         let res_token_client = TokenClient::new(&e, &reserve.config.b_token);
//         res_token_client.mint(&samwise, &0_5000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };
//             let user_emission_data = UserEmissionData {
//                 index: 56789,
//                 accrued: 0_1000000,
//             };

//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

//             update_user_emissions(
//                 &e,
//                 &reserve,
//                 res_token_type,
//                 &reserve_emission_data,
//                 &samwise,
//                 false,
//             )
//             .unwrap_optimized();

//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
//             assert_eq!(new_user_emission_data.accrued, 6_2700000);
//         });
//     }

//     #[test]
//     fn test_update_user_emissions_claim_returns_accrual() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 60_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         let res_token_client = TokenClient::new(&e, &reserve.config.b_token);
//         res_token_client.mint(&samwise, &0_5000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };
//             let user_emission_data = UserEmissionData {
//                 index: 56789,
//                 accrued: 0_1000000,
//             };

//             let res_token_type = 1;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             storage::set_user_emissions(&e, &samwise, &res_token_index, &user_emission_data);

//             let result = update_user_emissions(
//                 &e,
//                 &reserve,
//                 res_token_type,
//                 &reserve_emission_data,
//                 &samwise,
//                 true,
//             )
//             .unwrap_optimized();

//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
//             assert_eq!(new_user_emission_data.accrued, 0);
//             assert_eq!(result, 6_2700000);
//         });
//     }

//     #[test]
//     fn test_update_user_emissions_claim_first_time_claims_tokens() {
//         let e = Env::default();
//         e.mock_all_auths();

//         let pool_address = Address::random(&e);

//         let samwise = Address::random(&e);
//         let bombadil = Address::random(&e);

//         e.ledger().set(LedgerInfo {
//             timestamp: 1500000000,
//             protocol_version: 1,
//             sequence_number: 123,
//             network_id: Default::default(),
//             base_reserve: 10,
//         });

//         let mut reserve = create_reserve(&e);
//         reserve.data.b_supply = 100_0000000;
//         reserve.data.d_supply = 50_0000000;
//         setup_reserve(&e, &pool_address, &bombadil, &mut reserve);

//         let res_token_client = TokenClient::new(&e, &reserve.config.d_token);
//         res_token_client.mint(&samwise, &0_5000000);

//         e.as_contract(&pool_address, || {
//             let reserve_emission_data = ReserveEmissionsData {
//                 index: 123456789,
//                 last_time: 1500000000,
//             };

//             let res_token_type = 0;
//             let res_token_index = reserve.config.index * 3 + res_token_type;
//             let result = update_user_emissions(
//                 &e,
//                 &reserve,
//                 res_token_type,
//                 &reserve_emission_data,
//                 &samwise,
//                 true,
//             )
//             .unwrap_optimized();

//             let new_user_emission_data =
//                 storage::get_user_emissions(&e, &samwise, &res_token_index).unwrap_optimized();
//             assert_eq!(new_user_emission_data.index, reserve_emission_data.index);
//             assert_eq!(new_user_emission_data.accrued, 0);
//             assert_eq!(result, 6_1728394);
//         });
//     }
// }
