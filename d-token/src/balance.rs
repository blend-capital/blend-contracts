use soroban_sdk::{Address, Env};

use crate::{
    errors::TokenError,
    storage,
};

/// Spend "amount" of tokens from "user"
///
/// Errors if their is not enough balance to spend or the amount is negative
pub fn spend_balance(e: &Env, user: &Address, amount: &i128) -> Result<(), TokenError> {
    let mut balance = storage::read_balance(e, user);

    balance.amount -= amount;
    if balance.amount.is_negative() {
        return Err(TokenError::BalanceError);
    }
    storage::write_balance(e, user, &balance);
    Ok(())
}

/// Receive "amount" of tokens to "user"
///
/// Errors if their is not enough balance to spend or the amount is negative
pub fn receive_balance(e: &Env, user: &Address, amount: &i128) -> Result<(), TokenError> {
    let mut balance = storage::read_balance(e, user);

    balance.amount = balance
        .amount
        .checked_add(*amount)
        .ok_or(TokenError::OverflowError)?;
    storage::write_balance(e, user, &balance);
    Ok(())
}


#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        BytesN,
    };

    use crate::storage::Balance;

    use super::*;

    #[test]
    fn test_spend_balance() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let user = Address::random(&e);

        let starting_balance: i128 = 123456789;
        let amount: i128 = starting_balance - 1;
        e.as_contract(&token_id, || {
            storage::write_balance(
                &e,
                &user,
                &Balance {
                    amount: starting_balance,
                    authorized: true,
                },
            );

            spend_balance(&e, &user, &amount).unwrap();

            let balance = storage::read_balance(&e, &user);
            assert_eq!(balance.amount, 1);
        });
    }

    #[test]
    fn test_spend_balance_overspend_panics() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let user = Address::random(&e);

        let starting_balance: i128 = 123456789;
        let amount: i128 = starting_balance + 1;
        e.as_contract(&token_id, || {
            storage::write_balance(
                &e,
                &user,
                &Balance {
                    amount: starting_balance,
                    authorized: true,
                },
            );

            let result = spend_balance(&e, &user, &amount);
            assert_eq!(result, Err(TokenError::BalanceError));
        });
    }

    #[test]
    fn test_receive_balance() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let user = Address::random(&e);

        let amount: i128 = 123456789;
        e.as_contract(&token_id, || {
            receive_balance(&e, &user, &amount).unwrap();

            let balance = storage::read_balance(&e, &user);
            assert_eq!(balance.amount, amount);
        });
    }

    #[test]
    fn test_receive_balance_overflow_panics() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);
        let user = Address::random(&e);

        let amount: i128 = 123456789;
        e.as_contract(&token_id, || {
            receive_balance(&e, &user, &amount).unwrap();
            let result = receive_balance(&e, &user, &i128::MAX);
            assert_eq!(result, Err(TokenError::OverflowError));
        });
    }
}
