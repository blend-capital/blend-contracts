use soroban_auth::Identifier;
use soroban_sdk::{BigInt, BytesN, Env};

use crate::{
    dependencies::{OracleClient, TokenClient},
    storage::{PoolDataStore, StorageManager},
    user_config::{UserConfig, UserConfigurator},
};

/// A user's account data
pub struct UserData {
    pub e_collateral_base: BigInt, // user's effective collateral denominated in the base asset
    pub e_liability_base: BigInt,  // user's effective liability denominated in the base asset
}

pub struct UserAction {
    pub asset: BytesN<32>,
    pub b_token_delta: i64, // take protocol tokens in the event a rounding change occurs
    pub d_token_delta: i64,
}

impl UserData {
    pub fn load(e: &Env, user: &Identifier, action: &UserAction) -> UserData {
        let storage = StorageManager::new(e);
        let oracle_address = storage.get_oracle();
        let oracle_client = OracleClient::new(e, oracle_address);

        let user_config = UserConfig::new(storage.get_user_config(user.clone()));
        let reserve_count = storage.get_res_list();
        let mut e_collateral_base = BigInt::zero(e);
        let mut e_liability_base = BigInt::zero(e);
        for i in 0..reserve_count.len() {
            let res_asset_address = reserve_count.get_unchecked(i).unwrap();
            if !user_config.is_using_reserve(i) && res_asset_address != action.asset {
                continue;
            }

            let res_config = storage.get_res_config(res_asset_address.clone());
            let res_data = storage.get_res_data(res_asset_address.clone());
            let asset_to_base = BigInt::from_u64(&e, oracle_client.get_price(&res_asset_address));

            if user_config.is_collateral(i) {
                // append users effective collateral (after collateral factor) to e_collateral_base
                let b_token_client = TokenClient::new(e, res_config.b_token.clone());
                let b_token_balance = b_token_client.balance(user);
                e_collateral_base += to_effective_balance(
                    e,
                    b_token_balance,
                    BigInt::from_i64(e, res_data.b_rate),
                    BigInt::from_u32(e, res_config.c_factor),
                    asset_to_base.clone(),
                );
            }

            if user_config.is_borrowing(i) {
                // append users effective liability (after liability factor) to e_liability_base
                let d_token_client = TokenClient::new(e, res_config.d_token);
                let d_token_liability = d_token_client.balance(user);
                e_liability_base += to_effective_balance(
                    e,
                    d_token_liability,
                    BigInt::from_i64(e, res_data.d_rate),
                    BigInt::from_u64(e, 1_0000000_0000000)
                        / BigInt::from_u32(e, res_config.l_factor),
                    asset_to_base.clone(),
                );
            }

            // TODO: Change to i128 to allow negative e_foo_base numbers (https://github.com/stellar/rs-soroban-env/pull/570)
            //       Or find a way to support negative numbers
            if res_asset_address == action.asset {
                // user is making modifications to this asset, reflect them in the liability and/or collateral
                if action.b_token_delta != 0 {
                    let abs_delta = action.b_token_delta.abs();
                    let e_collateral_delta = to_effective_balance(
                        e,
                        BigInt::from_i64(e, abs_delta),
                        BigInt::from_i64(e, res_data.b_rate),
                        BigInt::from_u32(e, res_config.c_factor),
                        asset_to_base.clone(),
                    );
                    if action.b_token_delta > 0 {
                        e_collateral_base += e_collateral_delta.clone();
                    } else {
                        e_collateral_base -= e_collateral_delta;
                    }
                }

                if action.d_token_delta != 0 {
                    let abs_delta = action.d_token_delta.abs();
                    let e_liability_delta = to_effective_balance(
                        e,
                        BigInt::from_i64(e, abs_delta),
                        BigInt::from_i64(e, res_data.d_rate),
                        BigInt::from_u64(e, 1_0000000_0000000)
                            / BigInt::from_u32(e, res_config.l_factor),
                        asset_to_base.clone(),
                    );

                    if action.d_token_delta > 0 {
                        e_liability_base += e_liability_delta.clone();
                    } else {
                        e_liability_base -= e_liability_delta;
                    }
                }
            }
        }

        UserData {
            e_collateral_base,
            e_liability_base,
        }
    }
}

fn to_effective_balance(
    e: &Env,
    protocol_tokens: BigInt,
    rate: BigInt,
    ltv_factor: BigInt,
    oracle_price: BigInt,
) -> BigInt {
    let underlying = (protocol_tokens * rate) / BigInt::from_u64(e, 1_000_0000);
    let base = (underlying * oracle_price) / BigInt::from_u64(e, 1_000_0000);
    (base * ltv_factor) / BigInt::from_u64(e, 1_000_0000)
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{ReserveConfig, ReserveData},
        testutils::{create_mock_oracle, create_token_contract, generate_contract_id},
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::{testutils::Accounts, BigInt};

    // TODO: If moving from BigNum, add test for large nums
    #[test]
    fn test_to_effective_balance() {
        let e = Env::default();

        let protocol_tokens = BigInt::from_u64(&e, 1_000_000_0);
        let rate = BigInt::from_u64(&e, 1_234_567_8);
        let ltv_factor = BigInt::from_u64(&e, 0_777_777_7);
        let oracle_price = BigInt::from_u64(&e, 987_654_321_1);

        let expected_e_balance = BigInt::from_u64(&e, 948_364_7447);
        assert_eq!(
            to_effective_balance(&e, protocol_tokens, rate, ltv_factor, oracle_price),
            expected_e_balance
        );
    }

    #[test]
    fn test_load_user_only_collateral() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let collateral_amount = BigInt::from_u64(&e, 10_0000000);

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();

        // setup assets 0
        let (asset_id_0, _asset_0) = create_token_contract(&e, &bombadil);
        let (b_token_id_0, _b_token_0) = create_token_contract(&e, &bombadil);
        let (d_token_id_0, _d_token_0) = create_token_contract(&e, &bombadil);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_5000000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_0000000,
            d_rate: 1_1000000,
            ir_mod: 0,
        };

        // setup asset 1
        let (asset_id_1, _asset_1) = create_token_contract(&e, &bombadil);
        let (b_token_id_1, b_token_1) = create_token_contract(&e, &bombadil);
        let (d_token_id_1, _d_token_1) = create_token_contract(&e, &bombadil);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_7000000,
            l_factor: 0_6000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_1000000,
            d_rate: 1_2000000,
            ir_mod: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool_id, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &1000000_0000000);
        oracle_client.set_price(&asset_id_1, &5_0000000);

        // setup user (only collateralize asset 1)
        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x0000000000000008)
        });
        b_token_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &user_id,
            &collateral_amount,
        );

        // load user
        let user_action = UserAction {
            asset: BytesN::from_array(&e, &[0u8; 32]),
            d_token_delta: 0,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &user_id, &user_action);
            assert_eq!(user_data.e_liability_base, BigInt::zero(&e));
            assert_eq!(
                user_data.e_collateral_base,
                BigInt::from_u64(&e, 38_5000000)
            );
        });
    }

    #[test]
    fn test_load_user_only_liability() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);

        let liability_amount = BigInt::from_u64(&e, 12_0000000);

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());

        let bombadil = e.accounts().generate_and_create();

        // setup assets 0
        let (asset_id_0, _asset_0) = create_token_contract(&e, &bombadil);
        let (b_token_id_0, _b_token_0) = create_token_contract(&e, &bombadil);
        let (d_token_id_0, d_token_0) = create_token_contract(&e, &bombadil);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_5500000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_0000000,
            d_rate: 1_1000000,
            ir_mod: 0,
        };

        // setup asset 1
        let (asset_id_1, _asset_1) = create_token_contract(&e, &bombadil);
        let (b_token_id_1, _b_token_1) = create_token_contract(&e, &bombadil);
        let (d_token_id_1, _d_token_1) = create_token_contract(&e, &bombadil);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_7000000,
            l_factor: 0_6000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_1000000,
            d_rate: 1_2000000,
            ir_mod: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool_id, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &10_0000000);
        oracle_client.set_price(&asset_id_1, &0_0000001);

        // setup user (only liability asset 1)
        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x0000000000000001)
        });
        d_token_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &user_id,
            &liability_amount,
        );

        // load user
        let user_action = UserAction {
            asset: BytesN::from_array(&e, &[0u8; 32]),
            d_token_delta: 0,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &user_id, &user_action);
            assert_eq!(
                user_data.e_liability_base,
                BigInt::from_u64(&e, 239_9999976)
            ); // TODO: Rounding loss due to 1/l_factor taking floor
            assert_eq!(user_data.e_collateral_base, BigInt::zero(&e));
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

        // setup assets 0
        let (asset_id_0, _asset_0) = create_token_contract(&e, &bombadil);
        let (b_token_id_0, _b_token_0) = create_token_contract(&e, &bombadil);
        let (d_token_id_0, _d_token_0) = create_token_contract(&e, &bombadil);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_5500000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_0000000,
            d_rate: 1_1000000,
            ir_mod: 0,
        };

        // setup asset 1
        let (asset_id_1, _asset_1) = create_token_contract(&e, &bombadil);
        let (b_token_id_1, _b_token_1) = create_token_contract(&e, &bombadil);
        let (d_token_id_1, _d_token_1) = create_token_contract(&e, &bombadil);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_7000000,
            l_factor: 0_6000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_1000000,
            d_rate: 1_2000000,
            ir_mod: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool_id, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &10_0000000);
        oracle_client.set_price(&asset_id_1, &5_0000000);

        // setup user
        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x0000000000000000)
        });

        // load user
        let user_action = UserAction {
            asset: asset_id_0,
            d_token_delta: 0,
            b_token_delta: 3_0000000,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &user_id, &user_action);
            assert_eq!(user_data.e_liability_base, BigInt::zero(&e));
            assert_eq!(
                user_data.e_collateral_base,
                BigInt::from_u64(&e, 22_5000000)
            );
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

        // setup assets 0
        let (asset_id_0, _asset_0) = create_token_contract(&e, &bombadil);
        let (b_token_id_0, b_token_0) = create_token_contract(&e, &bombadil);
        let (d_token_id_0, _d_token_0) = create_token_contract(&e, &bombadil);
        let reserve_config_0 = ReserveConfig {
            b_token: b_token_id_0,
            d_token: d_token_id_0,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_5500000,
            util: 0_8000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 0,
        };
        let reserve_data_0 = ReserveData {
            b_rate: 1_0000000,
            d_rate: 1_1000000,
            ir_mod: 0,
        };

        // setup asset 1
        let (asset_id_1, _asset_1) = create_token_contract(&e, &bombadil);
        let (b_token_id_1, _b_token_1) = create_token_contract(&e, &bombadil);
        let (d_token_id_1, d_token_1) = create_token_contract(&e, &bombadil);
        let reserve_config_1 = ReserveConfig {
            b_token: b_token_id_1,
            d_token: d_token_id_1,
            decimals: 7,
            c_factor: 0_7000000,
            l_factor: 0_6000000,
            util: 0_7000000,
            r_one: 0,
            r_two: 0,
            r_three: 0,
            index: 1,
        };
        let reserve_data_1 = ReserveData {
            b_rate: 1_1000000,
            d_rate: 1_2000000,
            ir_mod: 0,
        };

        e.as_contract(&pool_id, || {
            storage.set_res_config(asset_id_0.clone(), reserve_config_0);
            storage.set_res_data(asset_id_0.clone(), reserve_data_0);
            storage.set_res_config(asset_id_1.clone(), reserve_config_1);
            storage.set_res_data(asset_id_1.clone(), reserve_data_1);
        });

        // setup oracle
        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        e.as_contract(&pool_id, || storage.set_oracle(oracle_id));
        oracle_client.set_price(&asset_id_0, &10_0000000);
        oracle_client.set_price(&asset_id_1, &5_0000000);

        // setup user
        let liability_amount = BigInt::from_u64(&e, 24_0000000);
        let collateral_amount = BigInt::from_u64(&e, 25_0000000);
        let additional_liability = -5_0000000;
        e.as_contract(&pool_id, || {
            storage.set_user_config(user_id.clone(), 0x0000000000000006)
        }); // ...01_10
        b_token_0.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &user_id,
            &collateral_amount,
        );
        d_token_1.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &BigInt::zero(&e),
            &user_id,
            &liability_amount,
        );

        // load user
        let user_action = UserAction {
            asset: asset_id_1,
            d_token_delta: additional_liability,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let user_data = UserData::load(&e, &user_id, &user_action);
            assert_eq!(
                user_data.e_liability_base,
                BigInt::from_u64(&e, 189_9999924)
            ); // TODO: same rounding loss as above
            assert_eq!(
                user_data.e_collateral_base,
                BigInt::from_u64(&e, 187_5000000)
            );
        });
    }
}
