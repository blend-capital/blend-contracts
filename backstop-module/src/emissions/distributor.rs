use cast::{i128, u64};
use fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, vec, Address, Env, Vec};

use crate::{
    constants::SCALAR_7,
    dependencies::TokenClient,
    errors::BackstopError,
    pool::Pool,
    storage::{self, BackstopEmissionConfig, BackstopEmissionsData, UserEmissionData},
    user::User,
};

pub fn distribute(e: &Env) {
    if e.ledger().timestamp() < storage::get_next_distribution(&e) {
        panic_with_error!(e, BackstopError::BadRequest);
    }
    let next_distribution = e.ledger().timestamp() + 7 * 24 * 60 * 60;
    storage::set_next_distribution(&e, &next_distribution);

    let reward_zone = storage::get_reward_zone(&e);
    let rz_len = reward_zone.len();
    let mut rz_tokens: Vec<i128> = vec![&e];

    // TODO: Potential to assume optimization of backstop token balances ~= RZ tokens
    //       However, linear iteration over the RZ will still occur
    // fetch total tokens of BLND in the reward zone
    let mut total_tokens: i128 = 0;
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone
            .get(rz_pool_index)
            .unwrap_optimized()
            .unwrap_optimized();
        let pool_tokens = storage::get_pool_tokens(&e, &rz_pool);
        rz_tokens.push_back(pool_tokens);
        total_tokens += i128(pool_tokens);
    }

    let blnd_token_client = TokenClient::new(e, &storage::get_blnd_token(e));
    // store pools EPS and distribute emissions to backstop depositors
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone
            .get(rz_pool_index)
            .unwrap_optimized()
            .unwrap_optimized();
        let cur_pool_tokens = i128(rz_tokens.pop_front_unchecked().unwrap_optimized());
        let share = cur_pool_tokens
            .fixed_div_floor(total_tokens, SCALAR_7)
            .unwrap_optimized();

        // store pool EPS and distribute pool's emissions via allowances to pool
        let pool_eps = share
            .fixed_mul_floor(0_3000000, SCALAR_7)
            .unwrap_optimized();
        let new_pool_emissions = pool_eps * 7 * 24 * 60 * 60;
        blnd_token_client.increase_allowance(
            &e.current_contract_address(),
            &rz_pool,
            &new_pool_emissions,
        );
        storage::set_pool_eps(&e, &rz_pool, &pool_eps);

        // distribute backstop depositor emissions
        let pool_backstop_eps = share
            .fixed_mul_floor(0_7000000, SCALAR_7)
            .unwrap_optimized();
        set_backstop_emission_config(
            e,
            &rz_pool,
            u64(pool_backstop_eps).unwrap_optimized(),
            next_distribution,
        );
    }
}

/// Set a new EPS for the backstop
pub fn set_backstop_emission_config(e: &Env, pool_id: &Address, eps: u64, expiration: u64) {
    let opt_emis_config = storage::get_backstop_emis_config(&e, pool_id);
    match opt_emis_config {
        Some(emis_config) => {
            // a previous config exists - update it before setting new EPS
            let total_shares = storage::get_pool_shares(e, pool_id);
            update_backstop_emission_index_with_config(e, pool_id, emis_config, total_shares);
        }
        None => {
            // first time the backstop is receiving emissions - ensure data is written
            storage::set_backstop_emis_data(
                e,
                pool_id,
                &BackstopEmissionsData {
                    index: 0,
                    last_time: e.ledger().timestamp(),
                },
            );
        }
    };
    let backstop_emis_config = BackstopEmissionConfig { expiration, eps };
    storage::set_backstop_emis_config(e, pool_id, &backstop_emis_config);
}

/// Update the backstop emissions index for deposits
pub fn update_backstop_emission_index(e: &Env, pool: &mut Pool) -> Option<i128> {
    if let Some(emis_config) = storage::get_backstop_emis_config(e, &pool.contract_id) {
        let total_shares = pool.get_shares(e);
        return update_backstop_emission_index_with_config(
            e,
            &pool.contract_id,
            emis_config,
            total_shares,
        );
    } else {
        return None;
    }
}

/// Update the backstop emissions index for deposits
fn update_backstop_emission_index_with_config(
    e: &Env,
    pool_id: &Address,
    emis_config: BackstopEmissionConfig,
    total_shares: i128,
) -> Option<i128> {
    let ledger_time = e.ledger().timestamp();

    let emis_data = storage::get_backstop_emis_data(e, &pool_id).unwrap_optimized(); // exists if config is written to
    if emis_data.last_time >= emis_config.expiration
        || e.ledger().timestamp() == emis_data.last_time
        || emis_config.eps == 0
        || total_shares == 0
    {
        // emis_data already updated or expired
        return Some(emis_data.index);
    }

    let max_timestamp = if ledger_time > emis_config.expiration {
        emis_config.expiration
    } else {
        ledger_time
    };

    let additional_idx = (i128(max_timestamp - emis_data.last_time) * i128(emis_config.eps))
        .fixed_div_floor(total_shares, SCALAR_7)
        .unwrap_optimized();
    let new_data = BackstopEmissionsData {
        index: additional_idx + emis_data.index,
        last_time: e.ledger().timestamp(),
    };
    storage::set_backstop_emis_data(e, &pool_id, &new_data);
    Some(new_data.index)
}

/// Update the backstop emissions index for the user and pool
///
/// Returns the number of tokens that need to be transfered to `user` when `to_claim`
/// is true, or returns zero.
pub fn update_emission_index(e: &Env, pool: &mut Pool, user: &mut User, to_claim: bool) -> i128 {
    if let Some(backstop_emis_index) = update_backstop_emission_index(e, pool) {
        let user_bal = user.get_shares(e);

        if let Some(user_data) = storage::get_user_emis_data(e, &pool.contract_id, &user.id) {
            if user_data.index != backstop_emis_index || to_claim {
                let mut accrual = user_data.accrued;
                if user_bal != 0 {
                    let to_accrue = user_bal
                        .fixed_mul_floor(backstop_emis_index - user_data.index, SCALAR_7)
                        .unwrap_optimized();
                    accrual += to_accrue;
                }
                return set_user_emissions(
                    e,
                    &pool.contract_id,
                    &user.id,
                    backstop_emis_index,
                    accrual,
                    to_claim,
                );
            }
            return 0;
        } else if user_bal == 0 {
            // first time the user registered an action with the asset since emissions were added
            return set_user_emissions(
                e,
                &pool.contract_id,
                &user.id,
                backstop_emis_index,
                0,
                to_claim,
            );
        } else {
            // user had tokens before emissions began, they are due any historical emissions
            let to_accrue = user_bal
                .fixed_mul_floor(backstop_emis_index, SCALAR_7)
                .unwrap_optimized();
            return set_user_emissions(
                e,
                &pool.contract_id,
                &user.id,
                backstop_emis_index,
                to_accrue,
                to_claim,
            );
        }
    }
    // else - no emissions need to be updated
    0
}

fn set_user_emissions(
    e: &Env,
    pool_id: &Address,
    user: &Address,
    index: i128,
    accrued: i128,
    to_claim: bool,
) -> i128 {
    if to_claim {
        storage::set_user_emis_data(e, pool_id, user, &UserEmissionData { index, accrued: 0 });
        return accrued;
    } else {
        storage::set_user_emis_data(e, pool_id, user, &UserEmissionData { index, accrued });
        return 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::{constants::BACKSTOP_EPOCH, testutils};

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec,
    };

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

        let bombadil = Address::random(&e);
        let backstop = Address::random(&e);
        let (_, blnd_token_client) = testutils::create_blnd_token(&e, &backstop, &bombadil);
        let pool_1 = Address::random(&e);
        let pool_2 = Address::random(&e);
        let pool_3 = Address::random(&e);
        let reward_zone: Vec<Address> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        let pool_1_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH,
            eps: 0_1000000,
        };
        let pool_1_emissions_data = BackstopEmissionsData {
            index: 887766,
            last_time: BACKSTOP_EPOCH - 12345,
        };
        e.as_contract(&backstop, || {
            storage::set_next_distribution(&e, &BACKSTOP_EPOCH);
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_backstop_emis_config(&e, &pool_1, &pool_1_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &pool_1_emissions_data);
            storage::set_pool_tokens(&e, &pool_1, &300_000_0000000);
            storage::set_pool_tokens(&e, &pool_2, &200_000_0000000);
            storage::set_pool_tokens(&e, &pool_3, &500_000_0000000);
            storage::set_pool_shares(&e, &pool_1, &300_000_0000000);
            storage::set_pool_shares(&e, &pool_2, &200_000_0000000);
            storage::set_pool_shares(&e, &pool_3, &500_000_0000000);
            blnd_token_client.increase_allowance(&backstop, &pool_1, &100_123_0000000);

            distribute(&e);

            assert_eq!(
                storage::get_next_distribution(&e),
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(storage::get_pool_tokens(&e, &pool_1), 300_000_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_2), 200_000_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_3), 500_000_0000000);
            assert_eq!(storage::get_pool_eps(&e, &pool_1), 0_0900000);
            assert_eq!(storage::get_pool_eps(&e, &pool_2), 0_0600000);
            assert_eq!(storage::get_pool_eps(&e, &pool_3), 0_1500000);
            assert_eq!(
                blnd_token_client.allowance(&backstop, &pool_1),
                154_555_0000000
            );
            assert_eq!(
                blnd_token_client.allowance(&backstop, &pool_2),
                36_288_0000000
            );
            assert_eq!(
                blnd_token_client.allowance(&backstop, &pool_3),
                90_720_0000000
            );
            let new_pool_1_config =
                storage::get_backstop_emis_config(&e, &pool_1).unwrap_optimized();
            let new_pool_1_data = storage::get_backstop_emis_data(&e, &pool_1).unwrap_optimized();
            assert_eq!(new_pool_1_config.eps, 0_2100000);
            assert_eq!(
                new_pool_1_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            // old config applied up to block timestamp
            assert_eq!(new_pool_1_data.index, 928916);
            assert_eq!(new_pool_1_data.last_time, BACKSTOP_EPOCH);
            let new_pool_2_config =
                storage::get_backstop_emis_config(&e, &pool_2).unwrap_optimized();
            let new_pool_2_data = storage::get_backstop_emis_data(&e, &pool_2).unwrap_optimized();
            assert_eq!(new_pool_2_config.eps, 0_1400000);
            assert_eq!(
                new_pool_2_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(new_pool_2_data.index, 0);
            assert_eq!(new_pool_2_data.last_time, BACKSTOP_EPOCH);
            let new_pool_3_config =
                storage::get_backstop_emis_config(&e, &pool_3).unwrap_optimized();
            let new_pool_3_data = storage::get_backstop_emis_data(&e, &pool_3).unwrap_optimized();
            assert_eq!(new_pool_3_config.eps, 0_3500000);
            assert_eq!(
                new_pool_3_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(new_pool_3_data.index, 0);
            assert_eq!(new_pool_3_data.last_time, BACKSTOP_EPOCH);
        });
    }

    #[test]
    #[should_panic(expected = "HostError\nValue: Status(ContractError(1))")]
    fn test_distribute_too_early() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let pool_1 = Address::random(&e);
        let pool_2 = Address::random(&e);
        let pool_3 = Address::random(&e);
        let reward_zone: Vec<Address> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        e.as_contract(&backstop_addr, || {
            storage::set_next_distribution(&e, &(BACKSTOP_EPOCH + 1));
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_pool_tokens(&e, &pool_1, &300_000_0000000);
            storage::set_pool_tokens(&e, &pool_2, &200_000_0000000);
            storage::set_pool_tokens(&e, &pool_3, &500_000_0000000);

            distribute(&e);
        });
    }

    /********** update_emission_index **********/

    #[test]
    fn test_update_emission_index() {
        let e = Env::default();
        let block_timestamp = BACKSTOP_EPOCH + 1234;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let pool_1 = Address::random(&e);
        let samwise = Address::random(&e);

        let backstop_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH + 7 * 24 * 60 * 60,
            eps: 0_1000000,
        };
        let backstop_emissions_data = BackstopEmissionsData {
            index: 22222,
            last_time: BACKSTOP_EPOCH,
        };
        let user_emissions_data = UserEmissionData {
            index: 11111,
            accrued: 3,
        };
        e.as_contract(&backstop_addr, || {
            storage::set_next_distribution(&e, &(BACKSTOP_EPOCH + 7 * 24 * 60 * 60));
            storage::set_backstop_emis_config(&e, &pool_1, &backstop_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &backstop_emissions_data);
            storage::set_user_emis_data(&e, &pool_1, &samwise, &user_emissions_data);

            storage::set_pool_tokens(&e, &pool_1, &200_0000000);
            storage::set_pool_shares(&e, &pool_1, &150_0000000);
            storage::set_shares(&e, &pool_1, &samwise, &9_0000000);

            let mut pool = Pool::new(&e, pool_1.clone());
            let mut user = User::new(pool_1.clone(), samwise.clone());
            let result = update_emission_index(&e, &mut pool, &mut user, false);
            let new_backstop_data = storage::get_backstop_emis_data(&e, &pool_1).unwrap_optimized();
            let new_user_data =
                storage::get_user_emis_data(&e, &pool_1, &samwise).unwrap_optimized();
            assert_eq!(result, 0);
            assert_eq!(new_backstop_data.last_time, block_timestamp);
            assert_eq!(new_backstop_data.index, 8248888);
            assert_eq!(new_user_data.accrued, 7_4139996);
            assert_eq!(new_user_data.index, 8248888);
        });
    }

    #[test]
    fn test_update_emission_index_no_config() {
        let e = Env::default();
        let block_timestamp = BACKSTOP_EPOCH + 1234;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let pool_1 = Address::random(&e);
        let samwise = Address::random(&e);

        e.as_contract(&backstop_addr, || {
            storage::set_next_distribution(&e, &(BACKSTOP_EPOCH + 7 * 24 * 60 * 60));

            storage::set_pool_tokens(&e, &pool_1, &200_0000000);
            storage::set_pool_shares(&e, &pool_1, &150_0000000);
            storage::set_shares(&e, &pool_1, &samwise, &9_0000000);

            let mut pool = Pool::new(&e, pool_1.clone());
            let mut user = User::new(pool_1.clone(), samwise.clone());
            let result = update_emission_index(&e, &mut pool, &mut user, false);
            let new_backstop_data = storage::get_backstop_emis_data(&e, &pool_1);
            let new_user_data = storage::get_user_emis_data(&e, &pool_1, &samwise);
            assert_eq!(result, 0);
            assert!(new_backstop_data.is_none());
            assert!(new_user_data.is_none());
        });
    }

    #[test]
    fn test_update_emission_index_to_claim() {
        let e = Env::default();
        let block_timestamp = BACKSTOP_EPOCH + 1234;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let pool_1 = Address::random(&e);
        let samwise = Address::random(&e);

        let backstop_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH + 7 * 24 * 60 * 60,
            eps: 0_1000000,
        };
        let backstop_emissions_data = BackstopEmissionsData {
            index: 22222,
            last_time: BACKSTOP_EPOCH,
        };
        let user_emissions_data = UserEmissionData {
            index: 11111,
            accrued: 3,
        };
        e.as_contract(&backstop_addr, || {
            storage::set_next_distribution(&e, &(BACKSTOP_EPOCH + 7 * 24 * 60 * 60));
            storage::set_backstop_emis_config(&e, &pool_1, &backstop_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &backstop_emissions_data);
            storage::set_user_emis_data(&e, &pool_1, &samwise, &user_emissions_data);

            storage::set_pool_tokens(&e, &pool_1, &200_0000000);
            storage::set_pool_shares(&e, &pool_1, &150_0000000);
            storage::set_shares(&e, &pool_1, &samwise, &9_0000000);

            let mut pool = Pool::new(&e, pool_1.clone());
            let mut user = User::new(pool_1.clone(), samwise.clone());
            let result = update_emission_index(&e, &mut pool, &mut user, true);
            let new_backstop_data = storage::get_backstop_emis_data(&e, &pool_1).unwrap_optimized();
            let new_user_data =
                storage::get_user_emis_data(&e, &pool_1, &samwise).unwrap_optimized();
            assert_eq!(result, 7_4139996);
            assert_eq!(new_backstop_data.last_time, block_timestamp);
            assert_eq!(new_backstop_data.index, 8248888);
            assert_eq!(new_user_data.accrued, 0);
            assert_eq!(new_user_data.index, 8248888);
        });
    }

    #[test]
    fn test_update_emission_index_first_action() {
        let e = Env::default();
        let block_timestamp = BACKSTOP_EPOCH + 12345;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let pool_1 = Address::random(&e);
        let samwise = Address::random(&e);

        let backstop_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH + 7 * 24 * 60 * 60,
            eps: 0_0420000,
        };
        let backstop_emissions_data = BackstopEmissionsData {
            index: 22222,
            last_time: BACKSTOP_EPOCH,
        };
        e.as_contract(&backstop_addr, || {
            storage::set_next_distribution(&e, &(BACKSTOP_EPOCH + 7 * 24 * 60 * 60));
            storage::set_backstop_emis_config(&e, &pool_1, &backstop_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &backstop_emissions_data);

            storage::set_pool_tokens(&e, &pool_1, &200_0000000);
            storage::set_pool_shares(&e, &pool_1, &150_0000000);

            let mut pool = Pool::new(&e, pool_1.clone());
            let mut user = User::new(pool_1.clone(), samwise.clone());
            let result = update_emission_index(&e, &mut pool, &mut user, true);
            let new_backstop_data = storage::get_backstop_emis_data(&e, &pool_1).unwrap_optimized();
            let new_user_data =
                storage::get_user_emis_data(&e, &pool_1, &samwise).unwrap_optimized();
            assert_eq!(result, 0);
            assert_eq!(new_backstop_data.last_time, block_timestamp);
            assert_eq!(new_backstop_data.index, 34588222);
            assert_eq!(new_user_data.accrued, 0);
            assert_eq!(new_user_data.index, 34588222);
        });
    }

    #[test]
    fn test_update_emission_index_config_set_after_user() {
        let e = Env::default();
        let block_timestamp = BACKSTOP_EPOCH + 12345;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_addr = Address::random(&e);
        let pool_1 = Address::random(&e);
        let samwise = Address::random(&e);

        let backstop_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH + 7 * 24 * 60 * 60,
            eps: 0_0420000,
        };
        let backstop_emissions_data = BackstopEmissionsData {
            index: 0,
            last_time: BACKSTOP_EPOCH,
        };
        e.as_contract(&backstop_addr, || {
            storage::set_next_distribution(&e, &(BACKSTOP_EPOCH + 7 * 24 * 60 * 60));
            storage::set_backstop_emis_config(&e, &pool_1, &backstop_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &backstop_emissions_data);

            storage::set_pool_tokens(&e, &pool_1, &200_0000000);
            storage::set_pool_shares(&e, &pool_1, &150_0000000);
            storage::set_shares(&e, &pool_1, &samwise, &9_0000000);

            let mut pool = Pool::new(&e, pool_1.clone());
            let mut user = User::new(pool_1.clone(), samwise.clone());
            let result = update_emission_index(&e, &mut pool, &mut user, false);
            let new_backstop_data = storage::get_backstop_emis_data(&e, &pool_1).unwrap_optimized();
            let new_user_data =
                storage::get_user_emis_data(&e, &pool_1, &samwise).unwrap_optimized();
            assert_eq!(result, 0);
            assert_eq!(new_backstop_data.last_time, block_timestamp);
            assert_eq!(new_backstop_data.index, 34566000);
            assert_eq!(new_user_data.accrued, 31_1094000);
            assert_eq!(new_user_data.index, 34566000);
        });
    }
}
