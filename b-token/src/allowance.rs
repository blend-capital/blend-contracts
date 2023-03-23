use soroban_sdk::{Address, Env};

use crate::{errors::TokenError, storage};

/// Increase the allowance of "spender" for "user" by "amount"
///
/// If the resulting amount is greater that i128::MAX, set to i128::MAX
pub fn increase_allowance(
    e: &Env,
    user: &Address,
    spender: &Address,
    amount: &i128,
) -> Result<(), TokenError> {
    let mut allowance = storage::read_allowance(&e, user, spender);
    allowance = allowance.checked_add(*amount).unwrap_or(i128::MAX);
    storage::write_allowance(e, user, spender, &allowance);
    Ok(())
}

/// Decrease the allowance of "spender" for "user" by "amount"
///
/// If the resulting amount is less than 0, set to 0
pub fn decrease_allowance(
    e: &Env,
    user: &Address,
    spender: &Address,
    amount: &i128,
) -> Result<(), TokenError> {
    let mut allowance = storage::read_allowance(&e, user, spender);
    allowance -= amount;
    if allowance.is_negative() {
        allowance = 0;
    }
    storage::write_allowance(e, user, spender, &allowance);
    Ok(())
}

/// Spend "amount" from the allowance of "spender" for "user"
///
/// Errors if the "spender" does not have enough allowance to spend "amount"
pub fn spend_allowance(
    e: &Env,
    user: &Address,
    spender: &Address,
    amount: &i128,
) -> Result<(), TokenError> {
    let mut allowance = storage::read_allowance(&e, user, spender);
    allowance -= amount;
    if allowance.is_negative() {
        return Err(TokenError::AllowanceError);
    }
    storage::write_allowance(e, user, spender, &allowance);
    Ok(())
}

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        BytesN,
    };

    use super::*;

    #[test]
    fn test_increase_allowance() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);

        let user = Address::random(&e);
        let spender = Address::random(&e);

        let amount: i128 = 123456789;
        e.as_contract(&token_id, || {
            increase_allowance(&e, &user, &spender, &amount).unwrap();

            let allowance = storage::read_allowance(&e, &user, &spender);
            assert_eq!(allowance, amount);
        });
    }

    #[test]
    fn test_increase_allowance_past_max_caps() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);

        let user = Address::random(&e);
        let spender = Address::random(&e);

        let amount: i128 = 123456789;
        e.as_contract(&token_id, || {
            storage::write_allowance(&e, &user, &spender, &amount);

            increase_allowance(&e, &user, &spender, &i128::MAX).unwrap();

            let allowance = storage::read_allowance(&e, &user, &spender);
            assert_eq!(allowance, i128::MAX);
        });
    }

    #[test]
    fn test_decrease_allowance() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);

        let user = Address::random(&e);
        let spender = Address::random(&e);

        let starting_allowance: i128 = 123456789;
        let amount: i128 = 987654;
        e.as_contract(&token_id, || {
            storage::write_allowance(&e, &user, &spender, &starting_allowance);

            decrease_allowance(&e, &user, &spender, &amount).unwrap();

            let allowance = storage::read_allowance(&e, &user, &spender);
            assert_eq!(allowance, starting_allowance - amount);
        });
    }

    #[test]
    fn test_decrease_allowance_past_0_caps() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);

        let user = Address::random(&e);
        let spender = Address::random(&e);

        let starting_allowance: i128 = 123456789;
        let amount: i128 = starting_allowance + 1;
        e.as_contract(&token_id, || {
            storage::write_allowance(&e, &user, &spender, &starting_allowance);

            decrease_allowance(&e, &user, &spender, &amount).unwrap();

            let allowance = storage::read_allowance(&e, &user, &spender);
            assert_eq!(allowance, 0);
        });
    }

    #[test]
    fn test_spend_allowance() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);

        let user = Address::random(&e);
        let spender = Address::random(&e);

        let starting_allowance: i128 = 123456789;
        let amount: i128 = 987654;
        e.as_contract(&token_id, || {
            storage::write_allowance(&e, &user, &spender, &starting_allowance);

            spend_allowance(&e, &user, &spender, &amount).unwrap();

            let allowance = storage::read_allowance(&e, &user, &spender);
            assert_eq!(allowance, starting_allowance - amount);
        });
    }

    #[test]
    fn test_spend_allowance_past_0_panics() {
        let e = Env::default();

        let token_id = BytesN::<32>::random(&e);

        let user = Address::random(&e);
        let spender = Address::random(&e);

        let starting_allowance: i128 = 123456789;
        let amount: i128 = starting_allowance + 1;
        e.as_contract(&token_id, || {
            storage::write_allowance(&e, &user, &spender, &starting_allowance);

            let result = spend_allowance(&e, &user, &spender, &amount);
            assert_eq!(result, Err(TokenError::AllowanceError));
        });
    }
}
