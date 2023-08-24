use crate::{
    contract::require_nonnegative,
    dependencies::{CometClient, TokenClient},
    storage,
};
use soroban_sdk::{Address, Env};

use super::require_is_from_pool_factory;

/// Perform a draw from a pool's backstop
pub fn execute_draw(e: &Env, pool_address: &Address, amount: i128, to: &Address) {
    require_nonnegative(e, amount);
    require_is_from_pool_factory(e, pool_address);

    let mut pool_balance = storage::get_pool_balance(e, pool_address);

    pool_balance.withdraw(e, amount, 0);
    storage::set_pool_balance(e, pool_address, &pool_balance);

    let backstop_token = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token.transfer(&e.current_contract_address(), to, &amount);
}

/// Perform a donation to a pool's backstop
pub fn execute_donate(e: &Env, from: &Address, pool_address: &Address, amount: i128) {
    require_nonnegative(e, amount);

    let backstop_token = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token.transfer(from, &e.current_contract_address(), &amount);

    let mut pool_balance = storage::get_pool_balance(e, pool_address);
    pool_balance.deposit(amount, 0);
    storage::set_pool_balance(e, pool_address, &pool_balance);
}

/// Perform a donation of USDC to a pool's backstop
pub fn execute_donate_usdc(e: &Env, from: &Address, pool_address: &Address, amount: i128) {
    require_nonnegative(e, amount);

    let usdc_token = TokenClient::new(e, &storage::get_usdc_token(e));
    usdc_token.transfer(from, &e.current_contract_address(), &amount);

    let mut pool_usdc = storage::get_pool_usdc(e, pool_address);
    pool_usdc += amount;
    storage::set_pool_usdc(e, pool_address, &pool_usdc);
}

/// Perform a mint of backstop LP tokens with USDC and deposit the LP tokens to the pool
pub fn execute_gulp_usdc(e: &Env, pool_address: &Address) {
    let pool_usdc = storage::get_pool_usdc(e, pool_address);
    if pool_usdc != 0 {
        // mint LP tokens with USDC
        let backstop_token = storage::get_backstop_token(e);
        let usdc_token = storage::get_usdc_token(e);
        let lp_tokens_minted = CometClient::new(e, &backstop_token)
            .dep_tokn_amt_in_get_lp_tokns_out(
                &usdc_token,
                &pool_usdc,
                &0,
                &e.current_contract_address(),
            );

        // attribute shares to the pool's backstop
        let mut pool_balance = storage::get_pool_balance(e, pool_address);
        pool_balance.deposit(lp_tokens_minted, 0);
        storage::set_pool_balance(e, pool_address, &pool_balance);
        storage::set_pool_usdc(e, pool_address, &0);
    }
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address};

    use crate::{
        backstop::execute_deposit,
        testutils::{
            create_backstop_token, create_blnd_token, create_comet_lp_pool,
            create_mock_pool_factory, create_usdc_token,
        },
    };

    use super::*;

    #[test]
    fn test_execute_donate() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);
        backstop_token_client.mint(&frodo, &100_0000000);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 25_0000000);
        });

        e.as_contract(&backstop_id, || {
            execute_donate(&e, &samwise, &pool_0_id, 30_0000000);

            let new_pool_balance = storage::get_pool_balance(&e, &pool_0_id);
            assert_eq!(new_pool_balance.shares, 25_0000000);
            assert_eq!(new_pool_balance.tokens, 55_0000000);
        });
    }

    #[test]
    // #[should_panic(expected = "ContractError(11)")]
    #[should_panic]
    fn test_execute_donate_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);
        backstop_token_client.mint(&frodo, &100_0000000);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 25_0000000);
        });

        e.as_contract(&backstop_id, || {
            execute_donate(&e, &samwise, &pool_0_id, -30_0000000);
        });
    }

    #[test]
    fn test_execute_draw() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_address = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_address);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000);
        });

        e.as_contract(&backstop_address, || {
            execute_draw(&e, &pool_0_id, 30_0000000, &samwise);

            let new_pool_balance = storage::get_pool_balance(&e, &pool_0_id);
            assert_eq!(new_pool_balance.shares, 50_0000000);
            assert_eq!(new_pool_balance.tokens, 20_0000000);
            assert_eq!(backstop_token_client.balance(&backstop_address), 20_0000000);
            assert_eq!(backstop_token_client.balance(&samwise), 30_0000000);
        });
    }

    #[test]
    #[should_panic]
    // #[should_panic(expected = "ContractError(10)")]
    fn test_execute_draw_requires_pool_factory_verification() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let pool_bad_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_id);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000);
        });

        e.as_contract(&backstop_id, || {
            execute_draw(&e, &pool_bad_id, 30_0000000, &samwise);
        });
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "HostError\nValue: Status(ContractError(6))")]
    fn test_execute_draw_only_can_take_from_pool() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let pool_1_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_id);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000);
            execute_deposit(&e, &frodo, &pool_1_id, 50_0000000);
        });

        e.as_contract(&backstop_id, || {
            execute_draw(&e, &pool_0_id, 51_0000000, &samwise);
        });
    }

    #[test]
    #[should_panic]
    // #[should_panic(expected = "ContractError(11)")]
    fn test_execute_draw_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&frodo, &100_0000000);

        let (_, mock_pool_factory_client) = create_mock_pool_factory(&e, &backstop_id);
        mock_pool_factory_client.set_pool(&pool_0_id);

        // initialize pool 0 with funds
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 50_0000000);
        });

        e.as_contract(&backstop_id, || {
            execute_draw(&e, &pool_0_id, -30_0000000, &samwise);
        });
    }

    #[test]
    fn test_execute_donate_usdc() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, usdc_token_client) = create_usdc_token(&e, &backstop_id, &bombadil);
        usdc_token_client.mint(&samwise, &100_0000000);
        usdc_token_client.mint(&frodo, &100_0000000);

        e.as_contract(&backstop_id, || {
            execute_donate_usdc(&e, &samwise, &pool_0_id, 30_0000000);
            let new_pool_usdc = storage::get_pool_usdc(&e, &pool_0_id);
            assert_eq!(new_pool_usdc, 30_0000000);
            assert_eq!(usdc_token_client.balance(&samwise), 70_0000000);
            assert_eq!(usdc_token_client.balance(&backstop_id), 30_0000000);

            execute_donate_usdc(&e, &frodo, &pool_0_id, 10_0000000);
            let new_pool_usdc = storage::get_pool_usdc(&e, &pool_0_id);
            assert_eq!(new_pool_usdc, 40_0000000);
            assert_eq!(usdc_token_client.balance(&frodo), 90_0000000);
            assert_eq!(usdc_token_client.balance(&backstop_id), 40_0000000);
        });
    }

    #[test]
    // #[should_panic(expected = "ContractError(11)")]
    #[should_panic]
    fn test_execute_donate_usdc_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, usdc_token_client) = create_usdc_token(&e, &backstop_id, &bombadil);
        usdc_token_client.mint(&samwise, &100_0000000);

        e.as_contract(&backstop_id, || {
            execute_donate_usdc(&e, &samwise, &pool_0_id, -30_0000000);
        });
    }

    #[test]
    fn test_execute_gulp_usdc() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited();

        let backstop_id = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        // let frodo = Address::random(&e);

        let (usdc_token, usdc_token_client) = create_usdc_token(&e, &backstop_id, &bombadil);
        usdc_token_client.mint(&samwise, &100_0000000);

        let (blnd_token, blnd_token_client) = create_blnd_token(&e, &backstop_id, &bombadil);
        blnd_token_client.mint(&samwise, &100_0000000);

        let (comet_id, comet_client) =
            create_comet_lp_pool(&e, &backstop_id, &bombadil, &blnd_token, &usdc_token);

        // initialize pool 0 with funds and a donation
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &bombadil, &pool_0_id, 10_0000000);
            execute_donate_usdc(&e, &samwise, &pool_0_id, 5_0000000);
            usdc_token_client.approve(&e.current_contract_address(), &comet_id, &i128::MAX, &500);
        });

        e.as_contract(&backstop_id, || {
            execute_gulp_usdc(&e, &pool_0_id);

            let new_pool_usdc = storage::get_pool_usdc(&e, &pool_0_id);
            assert_eq!(new_pool_usdc, 0);
            assert_eq!(usdc_token_client.balance(&backstop_id), 0);

            let new_pool_balance = storage::get_pool_balance(&e, &pool_0_id);
            assert_eq!(new_pool_balance.shares, 10_0000000);
            assert_eq!(new_pool_balance.tokens, 13_9904000);

            let comet_balance = comet_client.balance(&backstop_id);
            assert_eq!(comet_balance, 13_9904000);
        });
    }
}
