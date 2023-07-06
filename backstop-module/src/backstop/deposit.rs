use crate::{
    contract::require_nonnegative, dependencies::TokenClient, emissions, pool::Pool, storage,
    user::User,
};
use soroban_sdk::{Address, Env};

/// Perform a deposit into the backstop module
pub fn execute_deposit(e: &Env, from: &Address, pool_address: &Address, amount: i128) -> i128 {
    require_nonnegative(e, amount);
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    emissions::update_emission_index(e, &mut pool, &mut user, false);

    let to_mint = pool.convert_to_shares(e, amount);

    let backstop_token_client = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token_client.transfer(&from, &e.current_contract_address(), &amount);

    // "mint" shares
    pool.deposit(e, amount, to_mint);
    pool.write_shares(e);
    pool.write_tokens(e);

    user.add_shares(e, to_mint);
    user.write_shares(e);

    to_mint
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address};

    use crate::{backstop::execute_donate, testutils::create_backstop_token};

    use super::*;

    #[test]
    fn test_execute_deposit() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let pool_1_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);
        backstop_token_client.mint(&frodo, &100_0000000);

        // initialize pool 0 with funds + some profit
        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &frodo, &pool_0_id, 25_0000000);
            execute_donate(&e, &frodo, &pool_0_id, 25_0000000);
        });

        e.as_contract(&backstop_address, || {
            let shares_0 = execute_deposit(&e, &samwise, &pool_0_id, 30_0000000);
            let shares_1 = execute_deposit(&e, &samwise, &pool_1_id, 70_0000000);

            assert_eq!(shares_0, storage::get_shares(&e, &pool_0_id, &samwise));
            assert_eq!(shares_0, 15_0000000);
            assert_eq!(storage::get_pool_shares(&e, &pool_0_id), 40_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_0_id), 80_0000000);
            assert_eq!(storage::get_shares(&e, &pool_1_id, &samwise), shares_1);
            assert_eq!(shares_1, 70_0000000);
            assert_eq!(storage::get_pool_shares(&e, &pool_1_id), 70_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_1_id), 70_0000000);

            assert_eq!(
                backstop_token_client.balance(&backstop_address),
                150_0000000
            );
            assert_eq!(backstop_token_client.balance(&samwise), 0);
        });
    }

    #[test]
    #[should_panic]
    fn test_execute_deposit_too_many_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);

        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &samwise, &pool_0_id, 100_0000001);

            // TODO: Handle token errors gracefully
            assert!(false);
        });
    }

    #[test]
    #[should_panic(expected = "HostError\nValue: Status(ContractError(11))")]
    fn test_execute_deposit_negative_tokens() {
        let e = Env::default();
        e.mock_all_auths();

        let backstop_address = Address::random(&e);
        let pool_0_id = Address::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_address, &bombadil);
        backstop_token_client.mint(&samwise, &100_0000000);

        e.as_contract(&backstop_address, || {
            execute_deposit(&e, &samwise, &pool_0_id, -100);
        });
    }
}
