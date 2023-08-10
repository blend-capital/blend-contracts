use std::collections::HashMap;
use std::ops::Index;

use crate::backstop::create_backstop;
use crate::emitter::create_emitter;
use crate::mock_oracle::create_mock_oracle;
use crate::pool::POOL_WASM;
use crate::pool_factory::create_pool_factory;
use crate::token::{create_stellar_token, create_token, TokenClient};
use backstop_module::BackstopModuleClient;
use emitter::EmitterClient;
use lending_pool::{PoolClient, ReserveConfig};
use mock_oracle::MockOracleClient;
use pool_factory::{PoolFactoryClient, PoolInitMeta};
use soroban_sdk::testutils::{Address as _, BytesN as _, Ledger, LedgerInfo};
use soroban_sdk::{Address, BytesN, Env, Map, Symbol};

pub const SCALAR_7: i128 = 1_000_0000;
pub const SCALAR_9: i128 = 1_000_000_000;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum TokenIndex {
    BLND = 0,
    WETH = 1,
    USDC = 2,
    XLM = 3,
    BSTOP = 4,
}

pub struct PoolFixture<'a> {
    pub pool: PoolClient<'a>,
    pub reserves: HashMap<TokenIndex, u32>,
}

impl<'a> Index<TokenIndex> for Vec<TokenClient<'a>> {
    type Output = TokenClient<'a>;

    fn index(&self, index: TokenIndex) -> &Self::Output {
        &self[index as usize]
    }
}

pub struct TestFixture<'a> {
    pub env: Env,
    pub bombadil: Address,
    pub emitter: EmitterClient<'a>,
    pub backstop: BackstopModuleClient<'a>,
    pub pool_factory: PoolFactoryClient<'a>,
    pub oracle: MockOracleClient<'a>,
    pub pools: Vec<PoolFixture<'a>>,
    pub tokens: Vec<TokenClient<'a>>,
}

impl TestFixture<'_> {
    /// Create a new TestFixture for the Blend Protocol
    ///
    /// Deploys BLND (0), wETH (1), USDC (2), and XLM (3) test tokens, alongside all required
    /// Blend Protocol contracts, including the backstop token (4).
    pub fn create<'a>(wasm: bool) -> TestFixture<'a> {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let bombadil = Address::random(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1441065600, // Sept 1st, 2015 (backstop epoch)
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        // deploy tokens
        let (blnd_id, blnd_client) = create_token(&e, &bombadil, 7, "BLND");
        let (eth_id, eth_client) = create_token(&e, &bombadil, 9, "wETH");
        let (usdc_id, usdc_client) = create_token(&e, &bombadil, 6, "USDC");
        let (xlm_id, xlm_client) = create_stellar_token(&e, &bombadil); // TODO: make native

        // deploy Blend Protocol contracts
        let (backstop_id, backstop_client) = create_backstop(&e, wasm);
        let (emitter_id, emitter_client) = create_emitter(&e, wasm);
        let (pool_factory_id, _) = create_pool_factory(&e, wasm);
        let (_, mock_oracle_client) = create_mock_oracle(&e, wasm);

        // initialize emitter
        blnd_client.mint(&bombadil, &(10_000_000 * SCALAR_7));
        blnd_client.set_admin(&emitter_id);
        emitter_client.initialize(&backstop_id, &blnd_id);

        // initialize backstop
        let (backstop_token_id, backstop_token_client) = create_token(&e, &bombadil, 7, "BSTOP");
        backstop_client.initialize(
            &backstop_token_id,
            &blnd_id,
            &pool_factory_id,
            &Map::new(&e),
        );

        // initialize pool factory
        let pool_hash = e.deployer().upload_contract_wasm(POOL_WASM);
        let pool_init_meta = PoolInitMeta {
            backstop: backstop_id.clone(),
            pool_hash: pool_hash.clone(),
            blnd_id: blnd_id.clone(),
            usdc_id: usdc_id.clone(),
        };
        let pool_factory_client = PoolFactoryClient::new(&e, &pool_factory_id);
        pool_factory_client.initialize(&pool_init_meta);

        // initialize oracle
        mock_oracle_client.set_price(&blnd_id, &(0_0500000));
        mock_oracle_client.set_price(&backstop_token_id, &0_5000000);
        mock_oracle_client.set_price(&eth_id, &(2000_0000000));
        mock_oracle_client.set_price(&usdc_id, &(1_0000000));
        mock_oracle_client.set_price(&xlm_id, &(0_1000000));

        // pass 1 day
        e.ledger().set(LedgerInfo {
            timestamp: 1441152000,
            protocol_version: 1,
            sequence_number: 150,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        return TestFixture {
            env: e,
            bombadil,
            emitter: emitter_client,
            backstop: backstop_client,
            pool_factory: pool_factory_client,
            oracle: mock_oracle_client,
            pools: vec![],
            tokens: vec![
                blnd_client,
                eth_client,
                usdc_client,
                xlm_client,
                backstop_token_client,
            ],
        };
    }

    pub fn create_pool(&mut self, name: Symbol, backstop_take_rate: u64) {
        let pool_id = self.pool_factory.deploy(
            &self.bombadil,
            &name,
            &BytesN::<32>::random(&self.env),
            &self.oracle.address,
            &backstop_take_rate,
        );
        self.pools.push(PoolFixture {
            pool: PoolClient::new(&self.env, &pool_id),
            reserves: HashMap::new(),
        });
    }

    pub fn create_pool_reserve(
        &mut self,
        pool_index: usize,
        asset_index: TokenIndex,
        reserve_config: ReserveConfig,
    ) {
        let mut pool_fixture = self.pools.remove(pool_index);
        let token = &self.tokens[asset_index];
        pool_fixture
            .pool
            .init_reserve(&token.address, &reserve_config);
        let config = pool_fixture.pool.get_reserve_config(&token.address);
        pool_fixture.reserves.insert(asset_index, config.index);
        self.pools.insert(pool_index, pool_fixture);
    }

    /********** Chain Helpers ***********/

    pub fn jump(&self, time: u64) {
        let time = time.saturating_sub(self.env.ledger().timestamp());
        let blocks = time / 5;
        self.env.ledger().set(LedgerInfo {
            timestamp: self.env.ledger().timestamp() + time,
            protocol_version: 1,
            sequence_number: self.env.ledger().sequence() + (blocks as u32),
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
    }
}
