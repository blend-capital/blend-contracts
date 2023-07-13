use crate::{contract::require_nonnegative, dependencies::TokenClient, storage};
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
    backstop_token.transfer(&e.current_contract_address(), &to, &amount);
}

/// Perform a donation to a pool's backstop
pub fn execute_donate(e: &Env, from: &Address, pool_address: &Address, amount: i128) {
    require_nonnegative(e, amount);

    let backstop_token = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token.transfer(&from, &e.current_contract_address(), &amount);

    let mut pool_balance = storage::get_pool_balance(e, pool_address);
    pool_balance.deposit(amount, 0);
    storage::set_pool_balance(e, pool_address, &pool_balance);
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address};

    use crate::{
        backstop::execute_deposit,
        testutils::{create_backstop_token, create_mock_pool_factory},
    };

    use super::*;

    #[test]
    fn test_execute_donate() {
        let e = Env::default();
        e.mock_all_auths();

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
    #[should_panic(expected = "ContractError(11)")]
    fn test_execute_donate_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();

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
    #[should_panic(expected = "ContractError(10)")]
    fn test_execute_draw_requires_pool_factory_verification() {
        let e = Env::default();
        e.mock_all_auths();

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
    #[should_panic(expected = "HostError\nValue: Status(ContractError(6))")]
    fn test_execute_draw_only_can_take_from_pool() {
        let e = Env::default();
        e.mock_all_auths();

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
    #[should_panic(expected = "ContractError(11)")]
    fn test_execute_draw_negative_amount() {
        let e = Env::default();
        e.mock_all_auths();

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
}
