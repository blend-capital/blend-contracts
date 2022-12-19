use crate::{
    admin::{check_administrator, write_administrator},
    balance::{read_balance, receive_balance, spend_balance},
    errors::DTokenError,
    metadata::{has_metadata, read_decimal, read_name, read_symbol, write_metadata},
    public_types::{Metadata, TokenMetadata},
};
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{contractimpl, symbol, Bytes, Env};

/// ### d_token
///
/// d_token implementation

pub trait DTokenTrait {
    /// Initializes a d_token
    ///
    /// Arguments:
    /// * `admin`: The administrator of the d_token, this will always be the associated lending pool
    /// * `metadata`: The metadata of the d_token
    ///
    /// Errors:
    /// If the d_token has already been initialized, this function will panic with `DTokenError::AlreadyInitialized`
    fn init(e: Env, admin: Identifier, metadata: TokenMetadata) -> Result<(), DTokenError>;

    /// Gets the balance of the d_token for the given account
    ///
    /// Arguments:
    /// * `id`: The account to get the balance of
    fn balance(e: Env, id: Identifier) -> i128;

    /// Transfers tokens from one account to another
    ///
    /// Arguments:
    /// * `from`: The account to transfer tokens from
    /// * `to`: The account to transfer tokens to
    /// * `amount`: The amount of tokens to transfer
    ///
    /// Errors:
    /// If the function is called by a non-admin, this function will panic with `DTokenError::NotAuthorized`
    /// If the function is called with a negative number for `amount`, this function will panic with `DTokenError::NegativeNumber`
    fn xfer_from(e: Env, from: Identifier, to: Identifier, amount: i128)
        -> Result<(), DTokenError>;

    /// Mints tokens to a given account
    ///
    /// Arguments:
    /// * `to`: The account to mint tokens to
    /// * `amount`: The amount of tokens to mint
    ///
    /// Errors:
    /// If the function is called by a non-admin, this function will panic with `DTokenError::NotAuthorized`
    /// If the function is called with a negative number for `amount`, this function will panic with `DTokenError::NegativeNumber`
    fn mint(e: Env, to: Identifier, amount: i128) -> Result<(), DTokenError>;

    /// Burns tokens from a given account
    ///
    /// Arguments:
    /// * `from`: The account to burn tokens from
    /// * `amount`: The amount of tokens to burn
    ///
    /// Errors:
    /// If the function is called by a non-admin, this function will panic with `DTokenError::NotAuthorized`
    /// If the function is called with a negative number for `amount`, this function will panic with `DTokenError::NegativeNumber`
    fn burn(e: Env, from: Identifier, amount: i128) -> Result<(), DTokenError>;

    /// Returns the number of decimals the d_token uses
    fn decimals(e: Env) -> Result<u32, DTokenError>;

    /// Returns the name of the d_token
    fn name(e: Env) -> Result<Bytes, DTokenError>;

    /// Returns the symbol of the d_token
    fn symbol(e: Env) -> Result<Bytes, DTokenError>;
}

pub struct DToken;

fn check_nonnegative_amount(amount: i128) -> Result<(), DTokenError> {
    if amount < 0 {
        Err(DTokenError::NegativeNumber)
    } else {
        Ok(())
    }
}

#[contractimpl]
impl DTokenTrait for DToken {
    fn init(e: Env, admin: Identifier, metadata: TokenMetadata) -> Result<(), DTokenError> {
        if has_metadata(&e) {
            Err(DTokenError::AlreadyInitialized)
        } else {
            write_administrator(&e, admin);
            write_metadata(&e, Metadata::Token(metadata));
            Ok(())
        }
    }

    fn balance(e: Env, id: Identifier) -> i128 {
        read_balance(&e, id).unwrap() as i128
    }

    fn xfer_from(
        e: Env,
        from: Identifier,
        to: Identifier,
        amount: i128,
    ) -> Result<(), DTokenError> {
        check_nonnegative_amount(amount)?;
        //Only the admin may transfer tokens
        check_administrator(&e, &Signature::Invoker)?;
        spend_balance(&e, from.clone(), amount.clone() as u64)?;
        receive_balance(&e, to.clone(), amount.clone() as u64)?;
        e.events().publish((symbol!("transfer"), from, to), amount);
        Ok(())
    }

    fn burn(e: Env, from: Identifier, amount: i128) -> Result<(), DTokenError> {
        check_nonnegative_amount(amount)?;
        let admin_signature = Signature::Invoker;
        check_administrator(&e, &admin_signature)?;
        spend_balance(&e, from.clone(), amount.clone() as u64)?;
        e.events().publish(
            (symbol!("burn"), admin_signature.identifier(&e), from),
            amount,
        );
        Ok(())
    }

    fn mint(e: Env, to: Identifier, amount: i128) -> Result<(), DTokenError> {
        check_nonnegative_amount(amount)?;
        let admin_signature = Signature::Invoker;
        check_administrator(&e, &admin_signature)?;
        receive_balance(&e, to.clone(), amount.clone() as u64)?;
        e.events().publish(
            (symbol!("mint"), admin_signature.identifier(&e), to),
            amount,
        );
        Ok(())
    }

    fn decimals(e: Env) -> Result<u32, DTokenError> {
        read_decimal(&e)
    }

    fn name(e: Env) -> Result<Bytes, DTokenError> {
        read_name(&e)
    }

    fn symbol(e: Env) -> Result<Bytes, DTokenError> {
        read_symbol(&e)
    }
}
