use soroban_sdk::{testutils::BytesN as _, Address, BytesN, Env, Vec};

use crate::{
    b_token, backstop, d_token, emitter, mock_oracle,
    pool::{self, default_reserve_metadata, setup_reserve},
    pool_factory::{PoolFactoryClient, PoolInitMeta},
    token::TokenClient,
};

/// Initializes protocol contracts
///
/// ### Arguments
/// * `emitter_id` - The emitter contract ID
/// * `backstop_id` - The backstop contract ID
/// * `pool_factory_id` - The pool factory contract ID
/// * `blnd_id` - The blnd token contract ID
/// * `usdc_id` - The usdc token contract ID
/// * `backstop_token_id` - The backstop token contract ID
pub fn setup_protocol(
    e: &Env,
    emitter_id: BytesN<32>,
    backstop_id: BytesN<32>,
    pool_factory_id: BytesN<32>,
    blnd_id: BytesN<32>,
    usdc_id: BytesN<32>,
    backstop_token_id: BytesN<32>,
) {
    // initialize backstop
    let backstop_client = backstop::BackstopClient::new(&e, &backstop_id);
    backstop_client.initialize(&backstop_token_id, &blnd_id, &BytesN::<32>::random(&e));

    // initialize emitter
    let emitter_client = emitter::EmitterClient::new(&e, &emitter_id);
    emitter_client.initialize(&backstop_id, &blnd_id);
    // create metadata
    let wasm_hash = e.install_contract_wasm(pool::POOL_WASM);
    let b_token_hash = e.install_contract_wasm(b_token::B_TOKEN_WASM);
    let d_token_hash = e.install_contract_wasm(d_token::D_TOKEN_WASM);

    let pool_init_meta = PoolInitMeta {
        b_token_hash: b_token_hash.clone(),
        d_token_hash: d_token_hash.clone(),
        backstop: backstop_id.clone(),
        pool_hash: wasm_hash.clone(),
        blnd_id: blnd_id.clone(),
        usdc_id: usdc_id.clone(),
    };

    // initialize pool factory
    let pool_factory_client = PoolFactoryClient::new(&e, &pool_factory_id);
    pool_factory_client.initialize(&pool_init_meta);
}

fn mock_protocol(
    e: &Env,
    admin: Address,
    user: Address,
    usdc_id: BytesN<32>,
    backstop_token_id: BytesN<32>,
    emitter_id: BytesN<32>,
    backstop_id: BytesN<32>,
    pool_factory_id: BytesN<32>,
    oracle_id: BytesN<32>,
    asset_ids: Vec<BytesN<32>>,
    asset_prices: Vec<u64>,
    asset_c_factors: Vec<u32>,
    asset_l_factors: Vec<u32>,
    asset_utilizations: Vec<u32>,
) {
    //set asset prices
    let mock_oracle_client = mock_oracle::MockOracleClient::new(&e, &oracle_id);
    mock_oracle_client.set_price(&usdc_id, &1);
    for i in 0..(asset_ids.len() - 1) {
        let id = asset_ids.get(i).unwrap().unwrap();
        let price = asset_prices.get(i).unwrap().unwrap();
        mock_oracle_client.set_price(&id, &price);
    }

    //mint assets to user
    let usdc_client = TokenClient::new(&e, &usdc_id);
    usdc_client.mint(&admin, &user, &100_000_0000000);
    let backstop_token_client = TokenClient::new(&e, &backstop_token_id);
    backstop_token_client.mint(&admin, &user, &100_000_000_0000000);
    for i in 0..(asset_ids.len()) {
        let id = asset_ids.get(i).unwrap().unwrap();
        let asset_client = TokenClient::new(&e, &id);
        asset_client.mint(&admin, &user, &100_000_0000000);
    }

    //deploy and setup pool
    let pool_factory_client = PoolFactoryClient::new(&e, &pool_factory_id);
    let salt = BytesN::<32>::random(&e);
    let backstop_take_rate = 1000000;
    let pool_id = pool_factory_client.deploy(&admin, &salt, &oracle_id, &backstop_take_rate);
    let pool_client = pool::PoolClient::new(&e, &pool_id);
    let mut usdc_metadata = default_reserve_metadata();
    usdc_metadata.c_factor = 9000000;
    usdc_metadata.l_factor = 9500000;
    usdc_metadata.util = 8500000;
    setup_reserve(&pool_client, &admin, &usdc_metadata, &usdc_id);
    for i in 0..(asset_ids.len()) {
        let id = asset_ids.get(i).unwrap().unwrap();
        let c_factor = asset_c_factors.get(i).unwrap().unwrap();
        let l_factor = asset_l_factors.get(i).unwrap().unwrap();
        let util = asset_utilizations.get(i).unwrap().unwrap();
        let mut metadata = default_reserve_metadata();
        metadata.c_factor = c_factor;
        metadata.l_factor = l_factor;
        metadata.util = util;
        setup_reserve(&pool_client, &admin, &metadata, &id);
    }

    //set up backstop and enable pool
    let backstop_client = backstop::BackstopClient::new(&e, &backstop_id);
    backstop_client.deposit(&user, &pool_id, &10_000_000_0000000);
    pool_client.updt_stat();
    backstop_client.add_reward(&pool_id, &BytesN::from_array(&e, &[0u8; 32]));

    // distribute emissions
    let emitter_client = emitter::EmitterClient::new(&e, &emitter_id);
    emitter_client.distribute();

    //supply to and borrow from pool
    pool_client.supply(&user, &usdc_id, &100_000_0000000);
    for asset_ids in asset_ids.iter() {
        let id = asset_ids.unwrap();
        pool_client.supply(&user, &id, &10_000_0000000);
    }
    pool_client.borrow(&user, &usdc_id, &1_000_0000000, &user);
}

#[cfg(test)]
mod tests {

    use crate::{
        backstop::create_backstop, emitter::create_wasm_emitter, helpers::generate_contract_id,
        mock_oracle::create_mock_oracle, pool_factory::create_wasm_pool_factory,
        token::create_token,
    };
    use soroban_sdk::{
        testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
        vec,
    };

    use super::*;

    #[test]
    fn test_protocol_setup() {
        let e = Env::default();
        // disable limits for test
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let (emitter_id, emitter_client) = create_wasm_emitter(&e);
        let (backstop_id, backstop_client) = create_backstop(&e);
        let (pool_factory_id, pool_factory_client) = create_wasm_pool_factory(&e);
        let (blnd_id, _) = create_token(&e, &bombadil);
        let (usdc_id, _) = create_token(&e, &bombadil);
        let (backstop_token_id, _) = create_token(&e, &bombadil);

        setup_protocol(
            &e,
            emitter_id,
            backstop_id.clone(),
            pool_factory_id.clone(),
            blnd_id.clone(),
            usdc_id.clone(),
            backstop_token_id.clone(),
        );
        let res = emitter_client.try_initialize(&backstop_id, &blnd_id);
        match res {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
        assert_eq!(emitter_client.get_bstop(), backstop_id);
        let res1 = backstop_client.try_initialize(&backstop_token_id, &blnd_id, &pool_factory_id);
        match res1 {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
        assert_eq!(backstop_client.bstp_token(), backstop_token_id);
        let fake_init_data = PoolInitMeta {
            b_token_hash: generate_contract_id(&e),
            d_token_hash: generate_contract_id(&e),
            backstop: backstop_id.clone(),
            pool_hash: generate_contract_id(&e),
            blnd_id: blnd_id.clone(),
            usdc_id: usdc_id.clone(),
        };
        let res2 = pool_factory_client.try_initialize(&fake_init_data);
        match res2 {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
    }
    #[test]
    fn test_protocol_mock() {
        let e = Env::default();
        // disable limits for test
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: 1682439344,
            protocol_version: 1,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (emitter_id, _emitter_client) = create_wasm_emitter(&e);
        let (backstop_id, _backstop_client) = create_backstop(&e);
        let (pool_factory_id, _pool_factory_client) = create_wasm_pool_factory(&e);
        let (blnd_id, _) = create_token(&e, &Address::from_contract_id(&e, &emitter_id));
        let (usdc_id, _) = create_token(&e, &bombadil);
        let (backstop_token_id, _) = create_token(&e, &bombadil);

        setup_protocol(
            &e,
            emitter_id.clone(),
            backstop_id.clone(),
            pool_factory_id.clone(),
            blnd_id.clone(),
            usdc_id.clone(),
            backstop_token_id.clone(),
        );
        let (oracle_id, _) = create_mock_oracle(&e);
        let (asset1_id, _) = create_token(&e, &bombadil);
        let (asset2_id, _) = create_token(&e, &bombadil);
        let asset_ids = vec![&e, asset1_id, asset2_id];
        let asset_prices = vec![&e, 2_0000000, 4_0000000];
        let asset_c_factors = vec![&e, 7500000, 8500000];
        let asset_l_factors = vec![&e, 7500000, 8500000];
        let asset_utilizations = vec![&e, 5000000, 8500000];
        mock_protocol(
            &e,
            bombadil,
            samwise,
            usdc_id,
            backstop_token_id,
            emitter_id,
            backstop_id,
            pool_factory_id,
            oracle_id,
            asset_ids,
            asset_prices,
            asset_c_factors,
            asset_l_factors,
            asset_utilizations,
        )
    }
}
