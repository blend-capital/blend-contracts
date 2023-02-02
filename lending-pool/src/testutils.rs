#![cfg(any(test, feature = "testutils"))]

use crate::{
    constants::POOL_FACTORY,
    dependencies::{BackstopClient, TokenClient, BACKSTOP_WASM, TOKEN_WASM},
    reserve::Reserve,
    storage::{PoolDataStore, ReserveConfig, ReserveData, StorageManager},
};
use rand::{thread_rng, RngCore};
use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env, IntoVal};

pub(crate) fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

//************************************************
//           External Contract Helpers
//************************************************

//***** Token *****

pub(crate) fn create_token_contract(e: &Env, admin: &Identifier) -> (BytesN<32>, TokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, contract_id.clone());
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(&e));
    (contract_id, client)
}

pub(crate) fn create_token_from_id(
    e: &Env,
    contract_id: &BytesN<32>,
    admin: &Identifier,
) -> TokenClient {
    e.register_contract_wasm(contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, contract_id.clone());
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(&e));
    client
}

//***** Oracle ******
// TODO: Avoid WASM-ing unit tests by adding conditional `rlib` for test builds
//       -> https://rust-lang.github.io/rfcs/3180-cargo-cli-crate-type.html
// use mock_blend_oracle::testutils::register_test_mock_oracle;

mod mock_oracle {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm"
    );
}
pub(crate) use mock_oracle::Client as MockOracleClient;

pub(crate) fn create_mock_oracle(e: &Env) -> (BytesN<32>, MockOracleClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, mock_oracle::WASM);
    (contract_id.clone(), MockOracleClient::new(e, contract_id))
}

//***** Pool Factory ******

mod mock_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory::Client as MockPoolFactoryClient;

pub(crate) fn create_mock_pool_factory(e: &Env) -> MockPoolFactoryClient {
    let contract_id = BytesN::from_array(&e, &POOL_FACTORY);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    MockPoolFactoryClient::new(e, contract_id)
}

//***** Backstop ******

pub(crate) fn create_backstop(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, BACKSTOP_WASM);
    (contract_id.clone(), BackstopClient::new(e, contract_id))
}

//************************************************
//            Object Creation Helpers
//************************************************

//***** Reserve *****

pub(crate) fn create_reserve(e: &Env) -> Reserve {
    Reserve {
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
            reactivity: 0_000_010_000, // 10e-5
            index: 0,
        },
        data: ReserveData {
            b_rate: 1_000_000_000,
            d_rate: 1_000_000_000,
            ir_mod: 1_000_000_000,
            b_supply: 100_0000000,
            d_supply: 75_0000000,
            last_block: 0,
        },
    }
}

pub(crate) fn setup_reserve(
    e: &Env,
    pool_address: &BytesN<32>,
    admin: &Identifier,
    reserve: &Reserve,
) {
    let storage = StorageManager::new(e);
    e.as_contract(pool_address, || {
        storage.set_res_config(reserve.asset.clone(), reserve.config.clone());
        storage.set_res_data(reserve.asset.clone(), reserve.data.clone());
    });
    create_token_from_id(e, &reserve.asset, admin);
    let pool_id = Identifier::Contract(pool_address.clone());
    create_token_from_id(e, &reserve.config.b_token, &pool_id);
    create_token_from_id(e, &reserve.config.d_token, &pool_id);
}
