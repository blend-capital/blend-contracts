use crate::{dependencies::TokenClient, errors::BackstopError, pool::Pool, storage, user::User};
use soroban_sdk::{panic_with_error, Address, Env, Vec};

use super::update_emission_index;

// TODO: Deposit emissions back into the backstop automatically after 80/20 BLND deposit function added

/// Perform a claim for backstop deposit emissions by a user from the backstop module
pub fn execute_claim(e: &Env, from: &Address, pool_addresses: &Vec<Address>, to: &Address) -> i128 {
    if pool_addresses.len() == 0 {
        panic_with_error!(e, BackstopError::BadRequest);
    }

    let mut claimed: i128 = 0;
    for pool_addr in pool_addresses.iter_unchecked() {
        let mut pool = Pool::new(e, pool_addr.clone());
        let mut pool_user = User::new(pool_addr, from.clone());

        claimed += update_emission_index(e, &mut pool, &mut pool_user, true);
    }

    if claimed > 0 {
        let blnd_token = TokenClient::new(e, &storage::get_blnd_token(e));
        blnd_token.transfer(&e.current_contract_address(), &to, &claimed);
    }

    claimed
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{BackstopEmissionConfig, BackstopEmissionsData, UserEmissionData},
        testutils::create_blnd_token,
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        unwrap::UnwrapOptimized,
        vec,
    };

    /********** claim **********/

    #[test]
    fn test_claim() {
        let e = Env::default();
        e.mock_all_auths();
        let block_timestamp = 1500000000 + 12345;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_address = Address::random(&e);
        let pool_1_id = Address::random(&e);
        let pool_2_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, blnd_token_client) = create_blnd_token(&e, &backstop_address, &bombadil);
        blnd_token_client.mint(&backstop_address, &100_0000000);

        let backstop_1_emissions_config = BackstopEmissionConfig {
            expiration: 1500000000 + 7 * 24 * 60 * 60,
            eps: 0_1000000,
        };
        let backstop_1_emissions_data = BackstopEmissionsData {
            index: 22222,
            last_time: 1500000000,
        };
        let user_1_emissions_data = UserEmissionData {
            index: 11111,
            accrued: 1_2345678,
        };

        let backstop_2_emissions_config = BackstopEmissionConfig {
            expiration: 1500000000 + 7 * 24 * 60 * 60,
            eps: 0_0200000,
        };
        let backstop_2_emissions_data = BackstopEmissionsData {
            index: 0,
            last_time: 1500010000,
        };
        let user_2_emissions_data = UserEmissionData {
            index: 0,
            accrued: 0,
        };
        e.as_contract(&backstop_address, || {
            storage::set_backstop_emis_config(&e, &pool_1_id, &backstop_1_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1_id, &backstop_1_emissions_data);
            storage::set_user_emis_data(&e, &pool_1_id, &samwise, &user_1_emissions_data);
            storage::set_backstop_emis_config(&e, &pool_2_id, &backstop_2_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_2_id, &backstop_2_emissions_data);
            storage::set_user_emis_data(&e, &pool_2_id, &samwise, &user_2_emissions_data);

            storage::set_pool_tokens(&e, &pool_1_id, &200_0000000);
            storage::set_pool_shares(&e, &pool_1_id, &150_0000000);
            storage::set_shares(&e, &pool_1_id, &samwise, &9_0000000);
            storage::set_pool_tokens(&e, &pool_2_id, &75_0000000);
            storage::set_pool_shares(&e, &pool_2_id, &70_0000000);
            storage::set_shares(&e, &pool_2_id, &samwise, &7_5000000);

            let result = execute_claim(
                &e,
                &samwise,
                &vec![&e, pool_1_id.clone(), pool_2_id.clone()],
                &frodo,
            );
            assert_eq!(result, 75_3145677 + 5_0250000);
            assert_eq!(blnd_token_client.balance(&frodo), 75_3145677 + 5_0250000);
            assert_eq!(
                blnd_token_client.balance(&backstop_address),
                100_0000000 - (75_3145677 + 5_0250000)
            );

            let new_backstop_1_data =
                storage::get_backstop_emis_data(&e, &pool_1_id).unwrap_optimized();
            let new_user_1_data =
                storage::get_user_emis_data(&e, &pool_1_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_1_data.last_time, block_timestamp);
            assert_eq!(new_backstop_1_data.index, 82322222);
            assert_eq!(new_user_1_data.accrued, 0);
            assert_eq!(new_user_1_data.index, 82322222);

            let new_backstop_2_data =
                storage::get_backstop_emis_data(&e, &pool_2_id).unwrap_optimized();
            let new_user_2_data =
                storage::get_user_emis_data(&e, &pool_2_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_2_data.last_time, block_timestamp);
            assert_eq!(new_backstop_2_data.index, 6700000);
            assert_eq!(new_user_2_data.accrued, 0);
            assert_eq!(new_user_2_data.index, 6700000);
        });
    }

    #[test]
    fn test_claim_twice() {
        let e = Env::default();
        e.mock_all_auths();
        let block_timestamp = 1500000000 + 12345;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_address = Address::random(&e);
        let pool_1_id = Address::random(&e);
        let pool_2_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, blnd_token_client) = create_blnd_token(&e, &backstop_address, &bombadil);
        blnd_token_client.mint(&backstop_address, &200_0000000);

        let backstop_1_emissions_config = BackstopEmissionConfig {
            expiration: 1500000000 + 7 * 24 * 60 * 60,
            eps: 0_1000000,
        };
        let backstop_1_emissions_data = BackstopEmissionsData {
            index: 22222,
            last_time: 1500000000,
        };
        let user_1_emissions_data = UserEmissionData {
            index: 11111,
            accrued: 1_2345678,
        };

        let backstop_2_emissions_config = BackstopEmissionConfig {
            expiration: 1500000000 + 7 * 24 * 60 * 60,
            eps: 0_0200000,
        };
        let backstop_2_emissions_data = BackstopEmissionsData {
            index: 0,
            last_time: 1500010000,
        };
        let user_2_emissions_data = UserEmissionData {
            index: 0,
            accrued: 0,
        };
        e.as_contract(&backstop_address, || {
            storage::set_backstop_emis_config(&e, &pool_1_id, &backstop_1_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1_id, &backstop_1_emissions_data);
            storage::set_user_emis_data(&e, &pool_1_id, &samwise, &user_1_emissions_data);
            storage::set_backstop_emis_config(&e, &pool_2_id, &backstop_2_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_2_id, &backstop_2_emissions_data);
            storage::set_user_emis_data(&e, &pool_2_id, &samwise, &user_2_emissions_data);

            storage::set_pool_tokens(&e, &pool_1_id, &200_0000000);
            storage::set_pool_shares(&e, &pool_1_id, &150_0000000);
            storage::set_shares(&e, &pool_1_id, &samwise, &9_0000000);
            storage::set_pool_tokens(&e, &pool_2_id, &75_0000000);
            storage::set_pool_shares(&e, &pool_2_id, &70_0000000);
            storage::set_shares(&e, &pool_2_id, &samwise, &7_5000000);

            let result = execute_claim(
                &e,
                &samwise,
                &vec![&e, pool_1_id.clone(), pool_2_id.clone()],
                &frodo,
            );
            assert_eq!(result, 75_3145677 + 5_0250000);
            assert_eq!(blnd_token_client.balance(&frodo), 75_3145677 + 5_0250000);
            assert_eq!(
                blnd_token_client.balance(&backstop_address),
                200_0000000 - (75_3145677 + 5_0250000)
            );

            let new_backstop_1_data =
                storage::get_backstop_emis_data(&e, &pool_1_id).unwrap_optimized();
            let new_user_1_data =
                storage::get_user_emis_data(&e, &pool_1_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_1_data.last_time, block_timestamp);
            assert_eq!(new_backstop_1_data.index, 82322222);
            assert_eq!(new_user_1_data.accrued, 0);
            assert_eq!(new_user_1_data.index, 82322222);

            let new_backstop_2_data =
                storage::get_backstop_emis_data(&e, &pool_2_id).unwrap_optimized();
            let new_user_2_data =
                storage::get_user_emis_data(&e, &pool_2_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_2_data.last_time, block_timestamp);
            assert_eq!(new_backstop_2_data.index, 6700000);
            assert_eq!(new_user_2_data.accrued, 0);
            assert_eq!(new_user_2_data.index, 6700000);

            let block_timestamp_1 = 1500000000 + 12345 + 12345;
            e.ledger().set(LedgerInfo {
                timestamp: block_timestamp_1,
                protocol_version: 1,
                sequence_number: 0,
                network_id: Default::default(),
                base_reserve: 10,
            });
            let result_1 = execute_claim(
                &e,
                &samwise,
                &vec![&e, pool_1_id.clone(), pool_2_id.clone()],
                &frodo,
            );
            assert_eq!(result_1, 1005235710);
            assert_eq!(
                blnd_token_client.balance(&frodo),
                75_3145677 + 5_0250000 + 1005235710
            );
            assert_eq!(
                blnd_token_client.balance(&backstop_address),
                200_0000000 - (75_3145677 + 5_0250000) - (1005235710)
            );

            let new_backstop_1_data =
                storage::get_backstop_emis_data(&e, &pool_1_id).unwrap_optimized();
            let new_user_1_data =
                storage::get_user_emis_data(&e, &pool_1_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_1_data.last_time, block_timestamp_1);
            assert_eq!(new_backstop_1_data.index, 164622222);
            assert_eq!(new_user_1_data.accrued, 0);
            assert_eq!(new_user_1_data.index, 164622222);

            let new_backstop_2_data =
                storage::get_backstop_emis_data(&e, &pool_2_id).unwrap_optimized();
            let new_user_2_data =
                storage::get_user_emis_data(&e, &pool_2_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_2_data.last_time, block_timestamp_1);
            assert_eq!(new_backstop_2_data.index, 41971428);
            assert_eq!(new_user_2_data.accrued, 0);
            assert_eq!(new_user_2_data.index, 41971428);
        });
    }

    #[test]
    fn test_claim_no_deposits() {
        let e = Env::default();
        e.mock_all_auths();
        let block_timestamp = 1500000000 + 12345;
        e.ledger().set(LedgerInfo {
            timestamp: block_timestamp,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let backstop_address = Address::random(&e);
        let pool_1_id = Address::random(&e);
        let pool_2_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, blnd_token_client) = create_blnd_token(&e, &backstop_address, &bombadil);
        blnd_token_client.mint(&backstop_address, &100_0000000);

        let backstop_1_emissions_config = BackstopEmissionConfig {
            expiration: 1500000000 + 7 * 24 * 60 * 60,
            eps: 0_1000000,
        };
        let backstop_1_emissions_data = BackstopEmissionsData {
            index: 22222,
            last_time: 1500000000,
        };

        let backstop_2_emissions_config = BackstopEmissionConfig {
            expiration: 1500000000 + 7 * 24 * 60 * 60,
            eps: 0_0200000,
        };
        let backstop_2_emissions_data = BackstopEmissionsData {
            index: 0,
            last_time: 1500010000,
        };
        e.as_contract(&backstop_address, || {
            storage::set_backstop_emis_config(&e, &pool_1_id, &backstop_1_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1_id, &backstop_1_emissions_data);
            storage::set_backstop_emis_config(&e, &pool_2_id, &backstop_2_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_2_id, &backstop_2_emissions_data);

            storage::set_pool_tokens(&e, &pool_1_id, &200_0000000);
            storage::set_pool_shares(&e, &pool_1_id, &150_0000000);
            storage::set_pool_tokens(&e, &pool_2_id, &75_0000000);
            storage::set_pool_shares(&e, &pool_2_id, &70_0000000);

            let result = execute_claim(
                &e,
                &samwise,
                &vec![&e, pool_1_id.clone(), pool_2_id.clone()],
                &frodo,
            );
            assert_eq!(result, 0);
            assert_eq!(blnd_token_client.balance(&frodo), 0);
            assert_eq!(blnd_token_client.balance(&backstop_address), 100_0000000);

            let new_backstop_1_data =
                storage::get_backstop_emis_data(&e, &pool_1_id).unwrap_optimized();
            let new_user_1_data =
                storage::get_user_emis_data(&e, &pool_1_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_1_data.last_time, block_timestamp);
            assert_eq!(new_backstop_1_data.index, 82322222);
            assert_eq!(new_user_1_data.accrued, 0);
            assert_eq!(new_user_1_data.index, 82322222);

            let new_backstop_2_data =
                storage::get_backstop_emis_data(&e, &pool_2_id).unwrap_optimized();
            let new_user_2_data =
                storage::get_user_emis_data(&e, &pool_2_id, &samwise).unwrap_optimized();
            assert_eq!(new_backstop_2_data.last_time, block_timestamp);
            assert_eq!(new_backstop_2_data.index, 6700000);
            assert_eq!(new_user_2_data.accrued, 0);
            assert_eq!(new_user_2_data.index, 6700000);
        });
    }
}
