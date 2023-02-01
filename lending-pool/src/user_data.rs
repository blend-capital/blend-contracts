use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env};

use crate::{
    constants::SCALAR_7,
    dependencies::{OracleClient, TokenClient},
    reserve::Reserve,
    reserve_usage::ReserveUsage,
    storage::{PoolDataStore, StorageManager, PoolConfig},
};

/// A user's account data
pub struct UserData {
    /// The user's effective collateral balance denominated in the base asset
    pub collateral_base: i128,
    /// The user's effective liability balance denominated in the base asset
    pub liability_base: i128,
}

pub struct UserAction {
    pub asset: BytesN<32>,
    pub b_token_delta: i128, // take protocol tokens in the event a rounding change occurs
    pub d_token_delta: i128,
}

impl UserData {
    pub fn load(
        e: &Env,
        pool_config: &PoolConfig,
        user: &Identifier,
        action: &UserAction,
    ) -> UserData {
        let storage = StorageManager::new(e);
        let oracle_client = OracleClient::new(e, pool_config.oracle.clone());

        let user_config = ReserveUsage::new(storage.get_user_config(user.clone()));
        let reserve_count = storage.get_res_list();
        let mut collateral_base = 0;
        let mut liability_base = 0;
        for i in 0..reserve_count.len() {
            let res_asset_address = reserve_count.get_unchecked(i).unwrap();
            if !user_config.is_active_reserve(i) && res_asset_address != action.asset {
                continue;
            }

            let mut reserve = Reserve::load(&e, res_asset_address.clone());
            // do not write rate information to chain
            reserve.update_rates(e, pool_config.bstop_rate); 
            let asset_to_base = oracle_client.get_price(&res_asset_address);

            if user_config.is_collateral(i) {
                // append users effective collateral to collateral_base
                let b_token_client = TokenClient::new(e, reserve.config.b_token.clone());
                let b_token_balance = b_token_client.balance(user);
                let asset_collateral = reserve.to_effective_asset_from_b_token(b_token_balance);
                collateral_base += asset_collateral
                    .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                    .unwrap();
            }

            if user_config.is_liability(i) {
                // append users effective liability to liability_base
                let d_token_client = TokenClient::new(e, reserve.config.d_token.clone());
                let d_token_balance = d_token_client.balance(user);
                let asset_liability = reserve.to_effective_asset_from_d_token(d_token_balance);
                liability_base += asset_liability
                    .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                    .unwrap();
            }

            if res_asset_address == action.asset {
                // user is making modifications to this asset, reflect them in the liability and/or collateral
                if action.b_token_delta != 0 {
                    let asset_collateral =
                        reserve.to_effective_asset_from_b_token(action.b_token_delta);
                    collateral_base += asset_collateral
                        .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                        .unwrap();
                }

                if action.d_token_delta != 0 {
                    let asset_liability =
                        reserve.to_effective_asset_from_d_token(action.d_token_delta);
                    liability_base += asset_liability
                        .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                        .unwrap();
                }
            }
        }

        UserData {
            collateral_base,
            liability_base,
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        testutils::{create_mock_oracle, generate_contract_id, create_reserve, setup_reserve},
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::testutils::{Accounts, LedgerInfo, Ledger};

    #[test]
    fn test_load_user_only_collateral() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let collateral_amount = 10_0000000;

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let mut reserve_0 = create_reserve(&e);
        reserve_0.config.c_factor = 0_7500000;
        reserve_0.config.l_factor = 0_5000000;
        reserve_0.data.b_rate = 1_000_000_000;
        reserve_0.data.d_rate = 1_100_000_000;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.c_factor = 0_7000000;
        reserve_1.config.l_factor = 0_6000000;
        reserve_1.data.b_rate = 1_100_000_000;
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1000000_0000000);
        oracle_client.set_price(&reserve_1.asset, &5_0000000);

        // setup user (only collateralize reserve 1)
        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x0000000000000010);

            TokenClient::new(&e, &reserve_1.config.b_token).mint(
                &Signature::Invoker,
                &0,
                &user_id,
                &collateral_amount,
            );
        });

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0
        };
        
        let user_action = UserAction {
            asset: BytesN::from_array(&e, &[0u8; 32]),
            d_token_delta: 0,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &pool_config, &user_id, &user_action);
            assert_eq!(user_data.liability_base, 0);
            assert_eq!(user_data.collateral_base, 38_5000000);
        });
    }

    #[test]
    fn test_load_user_only_liability() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let liability_amount = 12_0000000;

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let mut reserve_0 = create_reserve(&e);
        reserve_0.config.c_factor = 0_7500000;
        reserve_0.config.l_factor = 0_5500000;
        reserve_0.data.b_rate = 1_000_000_000;
        reserve_0.data.d_rate = 1_100_000_000;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.c_factor = 0_7000000;
        reserve_1.config.l_factor = 0_6000000;
        reserve_1.data.b_rate = 1_100_000_000;
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &10_0000000);
        oracle_client.set_price(&reserve_1.asset, &0_0000001);

        // setup user (only liability reserve 0)
        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x0000000000000001);

            TokenClient::new(&e, &reserve_0.config.d_token).mint(
                &Signature::Invoker,
                &0,
                &user_id,
                &liability_amount,
            );
        });

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0
        };

        let user_action = UserAction {
            asset: BytesN::from_array(&e, &[0u8; 32]),
            d_token_delta: 0,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &pool_config, &user_id, &user_action);
            assert_eq!(user_data.liability_base, 240_0000000);
            assert_eq!(user_data.collateral_base, 0);
        });
    }

    #[test]
    fn test_load_user_only_action() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let mut reserve_0 = create_reserve(&e);
        reserve_0.config.c_factor = 0_7500000;
        reserve_0.config.l_factor = 0_5500000;
        reserve_0.data.b_rate = 1_000_000_000;
        reserve_0.data.d_rate = 1_100_000_000;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.c_factor = 0_7000000;
        reserve_1.config.l_factor = 0_6000000;
        reserve_1.data.b_rate = 1_100_000_000;
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &10_0000000);
        oracle_client.set_price(&reserve_1.asset, &5_0000000);

        // setup user with no positions
        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x0000000000000000)
        });

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0
        };

        let user_action = UserAction {
            asset: reserve_0.asset.clone(),
            d_token_delta: 0,
            b_token_delta: 3_0000000,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &pool_config, &user_id, &user_action);
            assert_eq!(user_data.liability_base, 0);
            assert_eq!(user_data.collateral_base, 22_5000000);
        });
    }

    #[test]
    fn test_load_user_all_positions() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let mut reserve_0 = create_reserve(&e);
        reserve_0.config.c_factor = 0_7500000;
        reserve_0.config.l_factor = 0_5500000;
        reserve_0.data.b_rate = 1_000_000_000;
        reserve_0.data.d_rate = 1_100_000_000;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.c_factor = 0_7000000;
        reserve_1.config.l_factor = 0_6000000;
        reserve_1.data.b_rate = 1_100_000_000;
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &10_0000000);
        oracle_client.set_price(&reserve_1.asset, &5_0000000);

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let liability_amount = 24_0000000;
        let collateral_amount = 25_0000000;
        let additional_liability = -5_0000000;

        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x000000000000000A);

            TokenClient::new(&e, &reserve_0.config.b_token).mint(
                &Signature::Invoker,
                &0,
                &user_id,
                &collateral_amount,
            );
            TokenClient::new(&e, &reserve_1.config.d_token).mint(
                &Signature::Invoker,
                &0,
                &user_id,
                &liability_amount,
            );
        });

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0
        };

        let user_action = UserAction {
            asset: reserve_1.asset.clone(),
            d_token_delta: additional_liability,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &pool_config, &user_id, &user_action);
            assert_eq!(user_data.liability_base, 190_0000000);
            assert_eq!(user_data.collateral_base, 187_5000000);
        });
    }

    #[test]
    fn test_load_user_updates_rates() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let mut reserve_0 = create_reserve(&e);
        reserve_0.config.c_factor = 0_7500000;
        reserve_0.config.l_factor = 0_5500000;
        reserve_0.data.b_rate = 1_000_000_000;
        reserve_0.data.d_rate = 1_100_000_000;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.c_factor = 0_7000000;
        reserve_1.config.l_factor = 0_6000000;
        reserve_1.data.b_rate = 1_100_000_000;
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil_id, &reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &10_0000000);
        oracle_client.set_price(&reserve_1.asset, &5_0000000);

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let liability_amount = 24_0000000;
        let collateral_amount = 25_0000000;
        let additional_liability = -5_0000000;

        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x000000000000000A);

            TokenClient::new(&e, &reserve_0.config.b_token).mint(
                &Signature::Invoker,
                &0,
                &user_id,
                &collateral_amount,
            );
            TokenClient::new(&e, &reserve_1.config.d_token).mint(
                &Signature::Invoker,
                &0,
                &user_id,
                &liability_amount,
            );
        });

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 123456,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        
        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0
        };

        let user_action = UserAction {
            asset: reserve_1.asset.clone(),
            d_token_delta: additional_liability,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &pool_config, &user_id, &user_action);
            assert_eq!(user_data.liability_base, 190_8570655);
            assert_eq!(user_data.collateral_base, 188_1744480);
        });
    }
}
