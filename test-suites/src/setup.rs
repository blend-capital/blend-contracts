use soroban_sdk::{testutils::Address as _, Address, Symbol};

use crate::{
    pool::{default_reserve_metadata, ReserveEmissionMetadata},
    test_fixture::{TestFixture, TokenIndex, SCALAR_7},
};

/// Create a test fixture with a pool and a whale depositing and borrowing all assets
pub fn create_fixture_with_data<'a>() -> (TestFixture<'a>, Address) {
    let mut fixture = TestFixture::create();
    fixture.env.mock_all_auths();
    // create pool
    fixture.create_pool(Symbol::new(&fixture.env, "Teapot"), 0_100_000_000);

    let mut usdc_meta = default_reserve_metadata();
    usdc_meta.c_factor = 0_900_0000;
    usdc_meta.l_factor = 0_950_0000;
    usdc_meta.util = 0_850_0000;
    fixture.create_pool_reserve(0, TokenIndex::USDC as usize, &usdc_meta);

    let mut xlm_meta = default_reserve_metadata();
    xlm_meta.c_factor = 0_750_0000;
    xlm_meta.l_factor = 0_750_0000;
    xlm_meta.util = 0_500_0000;
    fixture.create_pool_reserve(0, TokenIndex::XLM as usize, &xlm_meta);

    let mut weth_meta = default_reserve_metadata();
    weth_meta.c_factor = 0_800_0000;
    weth_meta.l_factor = 0_800_0000;
    weth_meta.util = 0_700_0000;
    fixture.create_pool_reserve(0, TokenIndex::WETH as usize, &weth_meta);

    // enable emissions for pool
    let pool_fixture = &fixture.pools[0];

    let reserve_emissions: soroban_sdk::Vec<ReserveEmissionMetadata> = soroban_sdk::vec![
        &fixture.env,
        ReserveEmissionMetadata {
            res_index: 0, // USDC
            res_type: 0,  // d_token
            share: 0_600_0000
        },
        ReserveEmissionMetadata {
            res_index: 1, // XLM
            res_type: 1,  // b_token
            share: 0_400_0000
        },
    ];
    pool_fixture
        .pool
        .set_emis(&fixture.bombadil, &reserve_emissions);

    // mint whale tokens
    let frodo = Address::random(&fixture.env);
    fixture.tokens[TokenIndex::USDC as usize].mint(&frodo, &(100_000 * SCALAR_7));
    fixture.tokens[TokenIndex::XLM as usize].mint(&frodo, &(1_000_000 * SCALAR_7));
    fixture.tokens[TokenIndex::WETH as usize].mint(&frodo, &(100 * SCALAR_7));
    fixture.tokens[TokenIndex::BSTOP as usize].mint(&frodo, &(10_000_000 * SCALAR_7));

    // deposit into backstop, add to reward zone
    fixture
        .backstop
        .deposit(&frodo, &pool_fixture.pool.address, &(2_000_000 * SCALAR_7));
    fixture
        .backstop
        .add_reward(&pool_fixture.pool.address, &Address::random(&fixture.env));
    pool_fixture.pool.updt_stat();

    // enable emissions
    fixture.emitter.distribute();
    fixture.backstop.distribute();
    pool_fixture.pool.updt_emis();

    fixture.jump(60);

    // supply and borrow all assets from whale
    pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(10_000 * SCALAR_7),
    );
    pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(100_000 * SCALAR_7),
    );

    pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::WETH as usize].address,
        &(10 * SCALAR_7),
    );

    pool_fixture.pool.borrow(
        &frodo,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(8_000 * SCALAR_7),
        &frodo,
    );
    pool_fixture.pool.borrow(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(65_000 * SCALAR_7),
        &frodo,
    );
    pool_fixture.pool.borrow(
        &frodo,
        &fixture.tokens[TokenIndex::WETH as usize].address,
        &(5 * SCALAR_7),
        &frodo,
    );

    fixture.jump(60 * 60); // 1 hr

    return (fixture, frodo);
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_create_fixture_with_data() {
        let (fixture, frodo) = create_fixture_with_data();
        let pool_fixture = &fixture.pools[0];

        // validate backstop deposit
        assert_eq!(
            2_000_000 * SCALAR_7,
            fixture.tokens[TokenIndex::BSTOP as usize].balance(&fixture.backstop.address)
        );

        // validate pool actions
        assert_eq!(
            2_000 * SCALAR_7,
            fixture.tokens[TokenIndex::USDC as usize].balance(&pool_fixture.pool.address)
        );
        assert_eq!(
            35_000 * SCALAR_7,
            fixture.tokens[TokenIndex::XLM as usize].balance(&pool_fixture.pool.address)
        );
        assert_eq!(
            5 * SCALAR_7,
            fixture.tokens[TokenIndex::WETH as usize].balance(&pool_fixture.pool.address)
        );

        assert_eq!(
            98_000 * SCALAR_7,
            fixture.tokens[TokenIndex::USDC as usize].balance(&frodo)
        );
        assert_eq!(
            965_000 * SCALAR_7,
            fixture.tokens[TokenIndex::XLM as usize].balance(&frodo)
        );
        assert_eq!(
            95 * SCALAR_7,
            fixture.tokens[TokenIndex::WETH as usize].balance(&frodo)
        );

        // validate emissions are turned on
        assert_eq!(
            0_300_0000,
            fixture.backstop.pool_eps(&pool_fixture.pool.address)
        );
        let (emis_config, _) = pool_fixture
            .pool
            .res_emis(&fixture.tokens[TokenIndex::USDC as usize].address, &0)
            .unwrap();
        assert_eq!(0_180_0000, emis_config.eps);
    }
}
