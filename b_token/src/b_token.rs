use crate::{
    admin::{check_administrator, write_administrator},
    allowance::{read_allowance, spend_allowance, write_allowance},
    balance::{read_balance, receive_balance, spend_balance},
    errors::TokenError,
    metadata::{has_metadata, read_decimal, read_name, read_symbol, write_metadata},
    nonce::{read_nonce, verify_and_consume_nonce},
    pool_reader::read_collateral,
    public_types::{Metadata, TokenMetadata},
};
use soroban_auth::{verify, Identifier, Signature};
use soroban_sdk::{contractimpl, symbol, Bytes, Env};

/// ### bToken
///
/// bToken implementation

pub trait BTokenTrait {
    /// Initializes a bToken
    ///
    /// Arguments:
    /// * `admin`: The administrator of the d_token, this will always be the associated lending pool
    /// * `metadata`: The metadata of the d_token
    ///
    /// Errors:
    /// If the b_token has already been initialized, this function will panic with `BTokenError::AlreadyInitialized`
    fn init(e: Env, admin: Identifier, metadata: TokenMetadata) -> Result<(), TokenError>;

    /// Gets the balance of the b_token for the given account
    ///
    /// Arguments:
    /// * `id`: The account to get the balance of
    fn balance(e: Env, id: Identifier) -> i128;

    /// Returns the nonce for an given account
    ///
    /// Arguments:
    /// * `id`: The account to get the nonce for
    fn nonce(e: Env, id: Identifier) -> i128;

    /// Returns a spenders allowance for a given account
    ///
    /// Arguments:
    /// * `from`: The account granting the allowance
    /// * `spender`: The account spending the allowance
    fn allowance(e: Env, from: Identifier, spender: Identifier) -> i128;

    /// Approves a spender to spend a given amount of tokens
    ///
    /// Arguments:
    /// * `from`: The account granting the allowance
    /// * `nonce`: The nonce of the account granting the allowance
    /// * `spender`: The account being allowed to spend the allowance
    /// * `amount`: The amount of tokens being allowed to be spent
    fn approve(
        e: Env,
        from: Signature,
        nonce: i128,
        spender: Identifier,
        amount: i128,
    ) -> Result<(), TokenError>;

    /// Transfers tokens from one account to another
    ///
    /// Arguments:
    /// * `from`: The account to transfer tokens from
    /// * `to`: The account to transfer tokens to
    /// * `amount`: The amount of tokens to transfer
    fn xfer(
        e: Env,
        from: Signature,
        nonce: i128,
        to: Identifier,
        amount: i128,
    ) -> Result<(), TokenError>;

    /// Transfers tokens from one account to another on behalf of a different account
    ///
    /// Arguments:
    /// * `from`: The account to transfer tokens from
    /// * `to`: The account to transfer tokens to
    /// * `amount`: The amount of tokens to transfer
    ///
    /// Errors:
    /// If the function is called by a non-admin, this function will panic with `BTokenError::NotAuthorized`
    /// If the function is called with a negative number for `amount`, this function will panic with `DTokenError::NegativeNumber`
    fn xfer_from(
        e: Env,
        spender: Signature,
        nonce: i128,
        from: Identifier,
        to: Identifier,
        amount: i128,
    ) -> Result<(), TokenError>;

    /// Checks if a b_token is being used as collateral by a given account
    ///
    /// Arguments:
    /// * `id`: The account to check whether they're using the b_token as collateral
    fn is_collat(e: Env, id: Identifier) -> bool;

    /// Mints tokens to a given account
    ///
    /// Arguments:
    /// * `to`: The account to mint tokens to
    /// * `amount`: The amount of tokens to mint
    ///
    /// Errors:
    /// If the function is called by a non-admin, this function will panic with `BTokenError::NotAuthorized`
    /// If the function is called with a negative number for `amount`, this function will panic with `BTokenError::NegativeNumber`
    fn mint(e: Env, to: Identifier, amount: i128) -> Result<(), TokenError>;

    /// Burns tokens from a given account
    ///
    /// Arguments:
    /// * `from`: The account to burn tokens from
    /// * `amount`: The amount of tokens to burn
    ///
    /// Errors:
    /// If the function is called by a non-admin, this function will panic with `BTokenError::NotAuthorized`
    /// If the function is called with a negative number for `amount`, this function will panic with `BTokenError::NegativeNumber`
    fn burn(e: Env, from: Identifier, amount: i128) -> Result<(), TokenError>;

    /// Returns the number of decimals the b_token uses
    fn decimals(e: Env) -> Result<u32, TokenError>;

    /// Returns the name of the b_token
    fn name(e: Env) -> Result<Bytes, TokenError>;

    /// Returns the symbol of the b_token
    fn symbol(e: Env) -> Result<Bytes, TokenError>;
}

pub struct BToken;

fn check_nonnegative_amount(amount: i128) -> Result<(), TokenError> {
    if amount < 0 {
        Err(TokenError::NegativeNumber)
    } else {
        Ok(())
    }
}

fn check_collateral(e: &Env, id: &Identifier) -> Result<(), TokenError> {
    if read_collateral(&e) {
        Err(TokenError::TokenCollateralized)
    } else {
        Ok(())
    }
}

#[contractimpl]
impl BTokenTrait for BToken {
    fn init(e: Env, admin: Identifier, metadata: TokenMetadata) -> Result<(), TokenError> {
        if has_metadata(&e) {
            return Err(TokenError::AlreadyInitialized);
        }

        write_administrator(&e, admin);
        write_metadata(&e, Metadata::Token(metadata));
        Ok(())
    }

    fn balance(e: Env, id: Identifier) -> i128 {
        read_balance(&e, id).unwrap() as i128
    }

    fn nonce(e: Env, id: Identifier) -> i128 {
        read_nonce(&e, id) as i128
    }

    fn allowance(e: Env, from: Identifier, spender: Identifier) -> i128 {
        read_allowance(&e, from, spender).unwrap() as i128
    }

    fn approve(
        e: Env,
        from: Signature,
        nonce: i128,
        spender: Identifier,
        amount: i128,
    ) -> Result<(), TokenError> {
        check_nonnegative_amount(amount)?;
        let from_id = from.identifier(&e);
        verify(
            &e,
            &from,
            symbol!("approve"),
            (from_id.clone(), nonce, spender.clone(), amount),
        );
        verify_and_consume_nonce(&e, &from, nonce)?;
        write_allowance(&e, from_id.clone(), spender.clone(), amount.clone() as u64);
        e.events()
            .publish((symbol!("approve"), from_id, spender), amount);

        Ok(())
    }

    fn xfer(
        e: Env,
        from: Signature,
        nonce: i128,
        to: Identifier,
        amount: i128,
    ) -> Result<(), TokenError> {
        check_nonnegative_amount(amount)?;
        let from_id = from.identifier(&e);
        // Collateralized tokens are non-transferrable
        check_collateral(&e, &from_id)?;
        verify(
            &e,
            &from,
            symbol!("xfer"),
            (from_id.clone(), nonce.clone(), to.clone(), amount.clone()),
        );
        verify_and_consume_nonce(&e, &from, nonce)?;
        spend_balance(&e, from_id.clone(), amount.clone())?;
        receive_balance(&e, to.clone(), amount.clone())?;
        e.events()
            .publish((symbol!("transfer"), from_id, to), amount);
        Ok(())
    }

    fn xfer_from(
        e: Env,
        spender: Signature,
        nonce: i128,
        from: Identifier,
        to: Identifier,
        amount: i128,
    ) -> Result<(), TokenError> {
        check_nonnegative_amount(amount)?;
        let spender_id = spender.identifier(&e);
        // Collateralized tokens are nontransferrable
        check_collateral(&e, &spender_id)?;
        verify(
            &e,
            &spender,
            symbol!("xfer_from"),
            (
                spender_id.clone(),
                nonce.clone(),
                to.clone(),
                amount.clone(),
            ),
        );
        verify_and_consume_nonce(&e, &spender, nonce)?;
        spend_allowance(&e, from.clone(), spender_id, amount)?;
        spend_balance(&e, from.clone(), amount.clone())?;
        receive_balance(&e, to.clone(), amount.clone())?;
        e.events().publish((symbol!("transfer"), from, to), amount);
        Ok(())
    }

    fn is_collat(e: Env, id: Identifier) -> bool {
        read_collateral(&e)
    }

    fn burn(e: Env, from: Identifier, amount: i128) -> Result<(), TokenError> {
        check_nonnegative_amount(amount.clone())?;
        let admin = Signature::Invoker;
        // No nonce verification as admin will always be a pool
        check_administrator(&e, &admin)?;
        let admin_id = admin.identifier(&e);
        spend_balance(&e, from.clone(), amount.clone())?;
        e.events()
            .publish((symbol!("burn"), admin_id, from), amount);
        Ok(())
    }

    fn mint(e: Env, to: Identifier, amount: i128) -> Result<(), TokenError> {
        check_nonnegative_amount(amount)?;
        let admin = Signature::Invoker;
        // No nonce verification as admin will always be a pool
        check_administrator(&e, &admin)?;
        let admin_id = admin.identifier(&e);
        receive_balance(&e, to.clone(), amount.clone())?;
        e.events().publish((symbol!("mint"), admin_id, to), amount);
        Ok(())
    }

    fn decimals(e: Env) -> Result<u32, TokenError> {
        read_decimal(&e)
    }

    fn name(e: Env) -> Result<Bytes, TokenError> {
        read_name(&e)
    }

    fn symbol(e: Env) -> Result<Bytes, TokenError> {
        read_symbol(&e)
    }
}
