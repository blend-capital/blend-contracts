use crate::{dependencies::TokenClient, errors::BackstopError, pool::Pool, storage, user::User};
use soroban_sdk::{Address, BytesN, Env};

/// Perform a deposit into the backstop module
pub fn execute_deposit(
    e: &Env,
    from: &Address,
    pool_address: &BytesN<32>,
    amount: i128,
) -> Result<i128, BackstopError> {
    let mut user = User::new(pool_address.clone(), from.clone());
    let mut pool = Pool::new(e, pool_address.clone());

    let to_mint = pool.convert_to_shares(e, amount);

    let backstop_token_client = TokenClient::new(e, &storage::get_backstop_token(e));
    backstop_token_client.xfer(&from, &e.current_contract_address(), &amount);

    // "mint" shares
    pool.deposit(e, amount, to_mint);
    pool.write_shares(e);
    pool.write_tokens(e);

    user.add_shares(e, to_mint);
    user.write_shares(e);

    Ok(to_mint)
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        Address, BytesN,
    };

    use crate::{backstop::execute_donate, testutils::create_backstop_token};

    use super::*;

    #[test]
    fn test_execute_deposit() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        let pool_0_id = BytesN::<32>::random(&e);
        let pool_1_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &100_0000000);
        backstop_token_client.mint(&bombadil, &frodo, &100_0000000);

        // initialize pool 0 with funds + some profit
        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &frodo, &pool_0_id, 25_0000000).unwrap();
            execute_donate(&e, &frodo, &pool_0_id, 25_0000000).unwrap();
        });

        e.as_contract(&backstop_id, || {
            let shares_0 = execute_deposit(&e, &samwise, &pool_0_id, 30_0000000).unwrap();
            let shares_1 = execute_deposit(&e, &samwise, &pool_1_id, 70_0000000).unwrap();

            assert_eq!(shares_0, storage::get_shares(&e, &pool_0_id, &samwise));
            assert_eq!(shares_0, 15_0000000);
            assert_eq!(storage::get_pool_shares(&e, &pool_0_id), 40_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_0_id), 80_0000000);
            assert_eq!(storage::get_shares(&e, &pool_1_id, &samwise), shares_1);
            assert_eq!(shares_1, 70_0000000);
            assert_eq!(storage::get_pool_shares(&e, &pool_1_id), 70_0000000);
            assert_eq!(storage::get_pool_tokens(&e, &pool_1_id), 70_0000000);

            assert_eq!(backstop_token_client.balance(&backstop), 150_0000000);
            assert_eq!(backstop_token_client.balance(&samwise), 0);
        });
    }

    #[test]
    #[should_panic]
    fn test_execute_deposit_too_many_tokens() {
        let e = Env::default();

        let backstop_id = BytesN::<32>::random(&e);
        let pool_0_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let (_, backstop_token_client) = create_backstop_token(&e, &backstop_id, &bombadil);
        backstop_token_client.mint(&bombadil, &samwise, &100_0000000);

        e.as_contract(&backstop_id, || {
            execute_deposit(&e, &samwise, &pool_0_id, 100_0000001).unwrap();

            // TODO: Handle token errors gracefully
            assert!(false);
        });
    }
}
