#![cfg(any(test, feature = "testutils"))]

use crate::{
    dependencies::{
        BackstopClient, BlendTokenClient, TokenClient, BACKSTOP_WASM, B_TOKEN_WASM, D_TOKEN_WASM,
        TOKEN_WASM,
    },
    reserve::Reserve,
    storage::{self, ReserveConfig, ReserveData},
};
use rand::{thread_rng, RngCore};
use soroban_sdk::{testutils::BytesN as _, Address, BytesN, Env, IntoVal};

pub(crate) fn generate_contract_id(e: &Env) -> BytesN<32> {
    let mut id: [u8; 32] = Default::default();
    thread_rng().fill_bytes(&mut id);
    BytesN::from_array(e, &id)
}

//************************************************
//           External Contract Helpers
//************************************************

//***** Token *****

pub(crate) fn create_token_contract(e: &Env, admin: &Address) -> (BytesN<32>, TokenClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, &contract_id);
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(e));
    (contract_id, client)
}

pub(crate) fn create_token_from_id(
    e: &Env,
    contract_id: &BytesN<32>,
    admin: &Address,
) -> TokenClient {
    e.register_contract_wasm(contract_id, TOKEN_WASM);
    let client = TokenClient::new(e, contract_id);
    client.initialize(admin, &7, &"unit".into_val(e), &"test".into_val(e));
    client
}

pub(crate) fn create_blnd_token(
    e: &Env,
    pool_id: &BytesN<32>,
    admin: &Address,
) -> (BytesN<32>, TokenClient) {
    let (contract_id, client) = create_token_contract(e, admin);

    e.as_contract(pool_id, || {
        storage::set_blnd_token(e, &contract_id);
    });
    (contract_id, client)
}

pub(crate) fn create_usdc_token(
    e: &Env,
    pool_id: &BytesN<32>,
    admin: &Address,
) -> (BytesN<32>, TokenClient) {
    let (contract_id, client) = create_token_contract(e, admin);

    e.as_contract(pool_id, || {
        storage::set_usdc_token(e, &contract_id);
    });
    (contract_id, client)
}

pub(crate) fn create_b_token_from_id(
    e: &Env,
    contract_id: &BytesN<32>,
    pool: &Address,
    pool_id: &BytesN<32>,
    asset: &BytesN<32>,
    res_index: u32,
) -> BlendTokenClient {
    e.register_contract_wasm(contract_id, B_TOKEN_WASM);
    let client = BlendTokenClient::new(e, contract_id);
    client.initialize(pool, &7, &"unit".into_val(e), &"test".into_val(e));
    client.init_asset(pool, pool_id, asset, &res_index);
    client
}

pub(crate) fn create_d_token_from_id(
    e: &Env,
    contract_id: &BytesN<32>,
    pool: &Address,
    pool_id: &BytesN<32>,
    asset: &BytesN<32>,
    res_index: u32,
) -> BlendTokenClient {
    e.register_contract_wasm(contract_id, D_TOKEN_WASM);
    let client = BlendTokenClient::new(e, contract_id);
    client.initialize(pool, &7, &"unit".into_val(e), &"test".into_val(e));
    client.init_asset(pool, pool_id, asset, &res_index);
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
    (contract_id.clone(), MockOracleClient::new(e, &contract_id))
}

//***** Pool Factory ******

mod mock_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/mock_pool_factory.wasm"
    );
}
pub use mock_pool_factory::Client as MockPoolFactoryClient;

pub(crate) fn create_mock_pool_factory(e: &Env) -> (BytesN<32>, MockPoolFactoryClient) {
    let contract_id = BytesN::<32>::random(e);
    e.register_contract_wasm(&contract_id, mock_pool_factory::WASM);
    (
        contract_id.clone(),
        MockPoolFactoryClient::new(e, &contract_id),
    )
}

//***** Backstop ******

pub(crate) fn create_backstop(e: &Env) -> (BytesN<32>, BackstopClient) {
    let contract_id = generate_contract_id(e);
    e.register_contract_wasm(&contract_id, BACKSTOP_WASM);
    (contract_id.clone(), BackstopClient::new(e, &contract_id))
}

pub(crate) fn setup_backstop(
    e: &Env,
    pool_id: &BytesN<32>,
    backstop_id: &BytesN<32>,
    backstop_token: &BytesN<32>,
    blnd_token: &BytesN<32>,
) {
    let (pool_factory, mock_pool_factory_client) = create_mock_pool_factory(e);
    mock_pool_factory_client.set_pool(pool_id);
    BackstopClient::new(e, backstop_id).initialize(backstop_token, blnd_token, &pool_factory);
    e.as_contract(pool_id, || {
        storage::set_backstop(e, backstop_id);
        storage::set_backstop_address(e, &Address::from_contract_id(e, backstop_id));
    });
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
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_010_000, // 10e-5
            index: 0,
        },
        data: ReserveData {
            d_rate: 1_000_000_000,
            ir_mod: 1_000_000_000,
            b_supply: 100_0000000,
            d_supply: 75_0000000,
            last_block: 0,
        },
        b_rate: Some(1_000_000_000),
    }
}

/// Expects a valid b_rate to be set
pub(crate) fn setup_reserve(e: &Env, pool_id: &BytesN<32>, admin: &Address, reserve: &mut Reserve) {
    e.as_contract(pool_id, || {
        let index = storage::push_res_list(e, &reserve.asset);
        reserve.config.index = index;
        storage::set_res_config(e, &reserve.asset, &reserve.config);
        storage::set_res_data(e, &reserve.asset, &reserve.data);
    });
    let asset_client = create_token_from_id(e, &reserve.asset, admin);
    let pool = Address::from_contract_id(e, pool_id);
    create_b_token_from_id(
        e,
        &reserve.config.b_token,
        &pool,
        pool_id,
        &reserve.asset,
        reserve.config.index,
    );
    create_d_token_from_id(
        e,
        &reserve.config.d_token,
        &pool,
        pool_id,
        &reserve.asset,
        reserve.config.index,
    );

    // mint pool assets to set expected b_rate
    let to_mint_pool = reserve.total_supply(e) - reserve.total_liabilities();
    asset_client.mint(admin, &pool, &to_mint_pool);
}
