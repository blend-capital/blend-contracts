use soroban_sdk::{Address, Env};

use crate::{errors::TokenError, interface::TokenClient, storage};

/// Spend "amount" of tokens from "user"
///
/// Errors if their is not enough balance to spend, if the resulting balance is negative
/// or if the user is not authorized by the underlying asset
pub fn spend_balance(e: &Env, user: &Address, amount: &i128) -> Result<(), TokenError> {
    is_authorized(e, user)?;
    spend_balance_no_auth(e, user, amount)
}

/// Receive "amount" of tokens to "user"
///
/// Errors if their is not enough balance to spend, if the amount is negative
pub fn receive_balance(e: &Env, user: &Address, amount: &i128) -> Result<(), TokenError> {
    is_authorized(e, user)?;

    let mut balance = storage::read_balance(e, user);
    balance = balance
        .checked_add(*amount)
        .ok_or(TokenError::OverflowError)?;
    storage::write_balance(e, user, &balance);
    Ok(())
}

/// Spend "amount" of tokens from "user"
///
/// Errors if their is not enough balance to spend, if the resulting balance is negative
/// or if the user is not authorized by the underlying asset
pub fn spend_balance_no_auth(e: &Env, user: &Address, amount: &i128) -> Result<(), TokenError> {
    let mut balance = storage::read_balance(e, user);
    balance -= amount;
    if balance.is_negative() {
        return Err(TokenError::BalanceError);
    }
    storage::write_balance(e, user, &balance);
    Ok(())
}

/// Check if user is authorized with underlying asest
fn is_authorized(e: &Env, user: &Address) -> Result<(), TokenError> {
    let underlying = storage::read_asset(&e);
    if !TokenClient::new(&e, &underlying.id).authorized(&user) {
        return Err(TokenError::BalanceDeauthorizedError);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        BytesN,
    };

    use crate::storage::Asset;

    use super::*;

    #[test]
    fn test_spend_balance() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let user = Address::random(&e);

        let underlying_id = e.register_stellar_asset_contract(bombadil.clone());
        TokenClient::new(&e, &underlying_id).set_auth(&bombadil, &user, &true);

        let starting_balance: i128 = 123456789;
        let amount: i128 = starting_balance - 1;
        e.as_contract(&token_id, || {
            storage::write_asset(
                &e,
                &Asset {
                    id: underlying_id.clone(),
                    res_index: 0,
                },
            );
            storage::write_balance(&e, &user, &starting_balance);

            spend_balance(&e, &user, &amount).unwrap();

            let balance = storage::read_balance(&e, &user);
            assert_eq!(balance, 1);
        });
    }

    #[test]
    fn test_spend_balance_overspend_panics() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let user = Address::random(&e);

        let underlying_id = e.register_stellar_asset_contract(bombadil.clone());
        TokenClient::new(&e, &underlying_id).set_auth(&bombadil, &user, &true);

        let starting_balance: i128 = 123456789;
        let amount: i128 = starting_balance + 1;
        e.as_contract(&token_id, || {
            storage::write_asset(
                &e,
                &Asset {
                    id: underlying_id.clone(),
                    res_index: 0,
                },
            );
            storage::write_balance(&e, &user, &starting_balance);

            let result = spend_balance(&e, &user, &amount);
            assert_eq!(result, Err(TokenError::BalanceError));
        });
    }

    #[test]
    fn test_spend_balance_deauthorized_panics() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let user = Address::random(&e);

        let underlying_id = e.register_stellar_asset_contract(bombadil.clone());
        TokenClient::new(&e, &underlying_id).set_auth(&bombadil, &user, &false);

        let starting_balance: i128 = 123456789;
        let amount: i128 = starting_balance - 1;
        e.as_contract(&token_id, || {
            storage::write_asset(
                &e,
                &Asset {
                    id: underlying_id.clone(),
                    res_index: 0,
                },
            );
            storage::write_balance(&e, &user, &starting_balance);

            let result = spend_balance(&e, &user, &amount);
            assert_eq!(result, Err(TokenError::BalanceDeauthorizedError));
        });
    }

    #[test]
    fn test_spend_balance_no_authorization() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let user = Address::random(&e);

        let underlying_id = e.register_stellar_asset_contract(bombadil.clone());
        TokenClient::new(&e, &underlying_id).set_auth(&bombadil, &user, &false);

        let starting_balance: i128 = 123456789;
        let amount: i128 = starting_balance - 1;
        e.as_contract(&token_id, || {
            storage::write_asset(
                &e,
                &Asset {
                    id: underlying_id.clone(),
                    res_index: 0,
                },
            );
            storage::write_balance(&e, &user, &starting_balance);

            spend_balance_no_auth(&e, &user, &amount).unwrap();
            let balance = storage::read_balance(&e, &user);
            assert_eq!(balance, 1);
        });
    }

    #[test]
    fn test_receive_balance() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let user = Address::random(&e);

        let underlying_id = e.register_stellar_asset_contract(bombadil.clone());
        TokenClient::new(&e, &underlying_id).set_auth(&bombadil, &user, &true);

        let amount: i128 = 123456789;
        e.as_contract(&token_id, || {
            storage::write_asset(
                &e,
                &Asset {
                    id: underlying_id.clone(),
                    res_index: 0,
                },
            );
            receive_balance(&e, &user, &amount).unwrap();

            let balance = storage::read_balance(&e, &user);
            assert_eq!(balance, amount);
        });
    }

    #[test]
    fn test_receive_balance_overflow_panics() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let user = Address::random(&e);

        let underlying_id = e.register_stellar_asset_contract(bombadil.clone());
        TokenClient::new(&e, &underlying_id).set_auth(&bombadil, &user, &true);

        let amount: i128 = 123456789;
        e.as_contract(&token_id, || {
            storage::write_asset(
                &e,
                &Asset {
                    id: underlying_id.clone(),
                    res_index: 0,
                },
            );
            receive_balance(&e, &user, &amount).unwrap();
            let result = receive_balance(&e, &user, &i128::MAX);
            assert_eq!(result, Err(TokenError::OverflowError));
        });
    }

    #[test]
    fn test_receive_balance_deauthorized_panics() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let user = Address::random(&e);

        let underlying_id = e.register_stellar_asset_contract(bombadil.clone());
        TokenClient::new(&e, &underlying_id).set_auth(&bombadil, &user, &false);

        e.as_contract(&token_id, || {
            storage::write_asset(
                &e,
                &Asset {
                    id: underlying_id.clone(),
                    res_index: 0,
                },
            );

            let result = receive_balance(&e, &user, &i128::MAX);
            assert_eq!(result, Err(TokenError::BalanceDeauthorizedError));
        });
    }
}
