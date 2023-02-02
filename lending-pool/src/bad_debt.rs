use soroban_auth::{Identifier, Signature};
use soroban_sdk::Env;

use crate::{
    dependencies::TokenClient,
    errors::PoolError,
    reserve::Reserve,
    reserve_usage::ReserveUsage,
    storage::{PoolDataStore, StorageManager},
};

/// Transfer bad debt from a user to the backstop. Validates that the user does hold bad debt
/// and transfers all held d_tokens to the backstop.
///
/// ### Arguments
/// * `user` - The user who has bad debt
///
/// ### Errors
/// If the user does not have bad debt
pub fn transfer_bad_debt_to_backstop(e: &Env, user: &Identifier) -> Result<(), PoolError> {
    let storage = StorageManager::new(&e);
    let user_res_config = ReserveUsage::new(storage.get_user_config(user.clone()));
    let has_collateral = user_res_config.has_collateral();
    let has_liability = user_res_config.has_liability();

    if has_collateral || !has_liability {
        return Err(PoolError::BadRequest);
    }

    // the user does not have collateral and currently holds a liability meaning they hold bad debt
    // transfer all of the user's debt to the backstop

    let pool_config = storage.get_pool_config();
    let backstop = Identifier::Contract(storage.get_backstop());
    let reserve_count = storage.get_res_list();
    for i in 0..reserve_count.len() {
        if !user_res_config.is_liability(i) {
            continue;
        }

        let res_asset_address = reserve_count.get_unchecked(i).unwrap();
        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        reserve.pre_action(e, &pool_config, 1, user.clone())?;

        let d_token_client = TokenClient::new(&e, &reserve.config.d_token);
        let user_balance = d_token_client.balance(user);
        d_token_client.clawback(&Signature::Invoker, &0, &user, &user_balance);
        d_token_client.mint(&Signature::Invoker, &0, &backstop, &user_balance);

        reserve.set_data(&e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::PoolConfig,
        testutils::{create_reserve, generate_contract_id, setup_reserve},
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::testutils::{Accounts, Ledger, LedgerInfo};

    #[test]
    fn test_transfer_bad_debt_happy_path() {
        let e = Env::default();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);
        let backstop = generate_contract_id(&e);
        let backstop_id = Identifier::Contract(backstop.clone());

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let pool_config = PoolConfig {
            oracle: generate_contract_id(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let liability_amount_0 = 24_0000000;
        let liability_amount_1 = 25_0000000;

        e.as_contract(&pool_id, || {
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop);
            let mut user_config = ReserveUsage::new(0);
            user_config.set_liability(0, true);
            user_config.set_liability(1, true);
            storage.set_user_config(user_id.clone(), user_config.config);

            let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);
            d_token_0.mint(&Signature::Invoker, &0, &user_id, &liability_amount_0);
            let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
            d_token_1.mint(&Signature::Invoker, &0, &user_id, &liability_amount_1);

            transfer_bad_debt_to_backstop(&e, &user_id).unwrap();

            assert_eq!(d_token_0.balance(&user_id), 0);
            assert_eq!(d_token_0.balance(&backstop_id), liability_amount_0);
            assert_eq!(d_token_1.balance(&user_id), 0);
            assert_eq!(d_token_1.balance(&backstop_id), liability_amount_1);

            let reserve_0_data = storage.get_res_data(reserve_0.asset);
            let reserve_1_data = storage.get_res_data(reserve_1.asset);
            assert_eq!(reserve_0_data.last_block, 123);
            assert_eq!(reserve_1_data.last_block, 123);
        });
    }

    #[test]
    fn test_transfer_bad_debt_with_collateral_errors() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);
        let backstop = generate_contract_id(&e);

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let pool_config = PoolConfig {
            oracle: generate_contract_id(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop);
            let mut user_config = ReserveUsage::new(0);
            user_config.set_liability(0, true);
            user_config.set_liability(1, true);
            user_config.set_supply(1, true);
            storage.set_user_config(user_id.clone(), user_config.config);

            let result = transfer_bad_debt_to_backstop(&e, &user_id);

            match result {
                Ok(_) => assert!(false),
                Err(error) => assert_eq!(error, PoolError::BadRequest),
            }
        });
    }

    #[test]
    fn test_transfer_bad_debt_without_liability_errors() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);
        let backstop = generate_contract_id(&e);

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let pool_config = PoolConfig {
            oracle: generate_contract_id(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop);
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(1, true);
            storage.set_user_config(user_id.clone(), user_config.config);

            let result = transfer_bad_debt_to_backstop(&e, &user_id);

            match result {
                Ok(_) => assert!(false),
                Err(error) => assert_eq!(error, PoolError::BadRequest),
            }
        });
    }
}
