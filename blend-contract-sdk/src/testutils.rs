use soroban_sdk::{token::StellarAssetClient, vec, Address, Env, Vec};

use crate::{backstop, emitter, pool, pool_factory};

pub mod comet {
    soroban_sdk::contractimport!(file = "./wasm/comet.wasm");
}

/// Create a "good enough" ReserveConfig for most testing usecases
///
/// Can be used when creating reserves for a pool.
pub fn default_reserve_config() -> pool::ReserveConfig {
    pool::ReserveConfig {
        decimals: 7,
        c_factor: 0_7500000,
        l_factor: 0_7500000,
        util: 0_7500000,
        max_util: 0_9500000,
        r_base: 0_0100000,
        r_one: 0_0500000,
        r_two: 0_5000000,
        r_three: 1_5000000,
        reactivity: 0_0000020, // 2e-6
        index: 0,
    }
}

/// Fixture for deploying and interacting with the Blend Protocol contracts in Rust tests.
pub struct BlendFixture<'a> {
    pub backstop: backstop::Client<'a>,
    pub emitter: emitter::Client<'a>,
    pub backstop_token: comet::Client<'a>,
    pub pool_factory: pool_factory::Client<'a>,
}

impl<'a> BlendFixture<'a> {
    /// Deploy a new set of Blend Protocol contracts. Mints 200k backstop
    /// tokens to the deployer that can be used in the future to create up to 4
    /// reward zone pools (50k tokens each).
    ///
    /// This function also resets the env budget via `reset_unlimited`.
    ///
    /// ### Arguments
    /// * `env` - The environment to deploy the contracts in
    /// * `deployer` - The address of the deployer
    /// * `blnd` - The address of the BLND token
    /// * `usdc` - The address of the USDC token
    pub fn deploy(
        env: &Env,
        deployer: &Address,
        blnd: &Address,
        usdc: &Address,
    ) -> BlendFixture<'a> {
        env.budget().reset_unlimited();
        let backstop = env.register_contract_wasm(None, backstop::WASM);
        let emitter = env.register_contract_wasm(None, emitter::WASM);
        let comet = env.register_contract_wasm(None, comet::WASM);
        let pool_factory = env.register_contract_wasm(None, pool_factory::WASM);
        let blnd_client = StellarAssetClient::new(env, &blnd);
        let usdc_client = StellarAssetClient::new(env, &usdc);
        blnd_client
            .mock_all_auths()
            .mint(deployer, &(1_000_0000000 * 2001));
        usdc_client
            .mock_all_auths()
            .mint(deployer, &(25_0000000 * 2001));

        let comet_client: comet::Client<'a> = comet::Client::new(env, &comet);
        comet_client.mock_all_auths().init(
            &deployer,
            &vec![env, blnd.clone(), usdc.clone()],
            &vec![env, 0_8000000, 0_2000000],
            &vec![env, 1_000_0000000, 25_0000000],
            &0_0030000,
        );

        comet_client.mock_all_auths().join_pool(
            &199_900_0000000, // finalize mints 100
            &vec![env, 1_000_0000000 * 2000, 25_0000000 * 2000],
            deployer,
        );

        blnd_client.mock_all_auths().set_admin(&emitter);
        let emitter_client: emitter::Client<'a> = emitter::Client::new(env, &emitter);
        emitter_client
            .mock_all_auths()
            .initialize(&blnd, &backstop, &comet);

        let backstop_client: backstop::Client<'a> = backstop::Client::new(env, &backstop);
        backstop_client.mock_all_auths().initialize(
            &comet,
            &emitter,
            &usdc,
            &blnd,
            &pool_factory,
            &Vec::new(env),
        );

        let pool_hash = env.deployer().upload_contract_wasm(pool::WASM);

        let pool_factory_client = pool_factory::Client::new(env, &pool_factory);
        pool_factory_client
            .mock_all_auths()
            .initialize(&pool_factory::PoolInitMeta {
                backstop,
                blnd_id: blnd.clone(),
                pool_hash,
            });
        backstop_client.update_tkn_val();

        BlendFixture {
            backstop: backstop_client,
            emitter: emitter_client,
            backstop_token: comet_client,
            pool_factory: pool_factory_client,
        }
    }
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        Address, BytesN, Env, String,
    };

    use crate::{
        pool,
        testutils::{default_reserve_config, BlendFixture},
    };

    #[test]
    fn test_deploy() {
        let env = Env::default();
        let deployer = Address::generate(&env);
        let blnd = env.register_stellar_asset_contract(deployer.clone());
        let usdc = env.register_stellar_asset_contract(deployer.clone());
        let blend = BlendFixture::deploy(&env, &deployer, &blnd, &usdc);
        assert_eq!(blend.backstop_token.balance(&deployer), 200_000_0000000);

        // deploy a pool, verify adding reserves, and backstop reward zone
        let token = env.register_stellar_asset_contract(deployer.clone());
        let pool = blend.pool_factory.mock_all_auths().deploy(
            &deployer,
            &String::from_str(&env, "test"),
            &BytesN::<32>::random(&env),
            &Address::generate(&env),
            &0_1000000, // 10%
            &4,         // 4 max positions
        );
        let pool_client = pool::Client::new(&env, &pool);
        let reserve_config = default_reserve_config();
        pool_client
            .mock_all_auths()
            .queue_set_reserve(&token, &reserve_config);
        pool_client.mock_all_auths().set_reserve(&token);

        blend
            .backstop
            .mock_all_auths()
            .deposit(&deployer, &pool, &50_000_0000000);
        pool_client.mock_all_auths().set_status(&3); // remove pool from setup status
        pool_client.mock_all_auths().update_status();

        assert_eq!(pool_client.update_status(), 1); // pool is active
        assert!(blend.pool_factory.is_pool(&pool)); // pool factory knows about the pool
    }
}
