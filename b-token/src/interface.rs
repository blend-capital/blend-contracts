use soroban_sdk::{contractclient, Address, Bytes, BytesN, Env};

use crate::storage::Asset;

/// A basic interface that allows the transfer and storage of tokens.
///
/// Based on https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-06.md
#[contractclient(name = "TokenClient")]
pub trait CAP4606 {
    // --------------------------------------------------------------------------------
    // Soroban specific interface
    // --------------------------------------------------------------------------------

    /// Initialize the token contract with an admin and token metadata
    fn initialize(e: Env, admin: Address, decimal: u32, name: Bytes, symbol: Bytes);

    // --------------------------------------------------------------------------------
    // Admin interface – privileged functions.
    // --------------------------------------------------------------------------------
    //
    // All the admin functions have to be authorized by the admin with all input
    // arguments, i.e. they have to call `admin.require_auth()`.

    /// If "admin" is the administrator, clawback "amount" from "from". "amount" is burned.
    /// Emit event with topics = ["clawback", admin: Address, to: Address], data = [amount: i128]
    fn clawback(env: Env, admin: Address, from: Address, amount: i128);

    /// If "admin" is the administrator, mint "amount" to "to".
    /// Emit event with topics = ["mint", admin: Address, to: Address], data = [amount: i128]
    fn mint(env: Env, admin: Address, to: Address, amount: i128);

    /// If "admin" is the administrator, set the administrator to "id".
    /// Emit event with topics = ["set_admin", admin: Address], data = [new_admin: Address]
    fn set_admin(env: Env, admin: Address, new_admin: Address);

    /// If "admin" is the administrator, set the authorize state of "id" to "authorize".
    /// If "authorize" is true, "id" should be able to use its balance.
    /// Emit event with topics = ["set_auth", admin: Address, id: Address], data = [authorize: bool]
    fn set_auth(env: Env, admin: Address, id: Address, authorize: bool);

    // --------------------------------------------------------------------------------
    // Token interface
    // --------------------------------------------------------------------------------
    //
    // All the functions here have to be authorized by the token spender
    // (usually named `from` here) using all the input arguments, i.e. they have
    // to call  `from.require_auth()`.

    /// Increase the allowance by "amount" for "spender" to transfer/burn from "from".
    /// Emit event with topics = ["incr_allow", from: Address, spender: Address], data = [amount: i128]
    fn incr_allow(env: Env, from: Address, spender: Address, amount: i128);

    /// Decrease the allowance by "amount" for "spender" to transfer/burn from "from".
    /// If "amount" is greater than the current allowance, set the allowance to 0.
    /// Emit event with topics = ["decr_allow", from: Address, spender: Address], data = [amount: i128]
    fn decr_allow(env: Env, from: Address, spender: Address, amount: i128);

    /// Transfer "amount" from "from" to "to.
    /// Emit event with topics = ["transfer", from: Address, to: Address], data = [amount: i128]
    fn xfer(env: Env, from: Address, to: Address, amount: i128);

    /// Transfer "amount" from "from" to "to", consuming the allowance of "spender".
    /// Authorized by spender (`spender.require_auth()`).
    /// Emit event with topics = ["transfer", from: Address, to: Address], data = [amount: i128]
    fn xfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128);

    /// Burn "amount" from "from".
    /// Emit event with topics = ["burn", from: Address], data = [amount: i128]
    fn burn(env: Env, from: Address, amount: i128);

    /// Burn "amount" from "from", consuming the allowance of "spender".
    /// Emit event with topics = ["burn", from: Address], data = [amount: i128]
    fn burn_from(env: Env, spender: Address, from: Address, amount: i128);

    // --------------------------------------------------------------------------------
    // Read-only Token interface
    // --------------------------------------------------------------------------------
    //
    // The functions here don't need any authorization and don't emit any
    // events.

    /// Get the balance of "id".
    fn balance(env: Env, id: Address) -> i128;

    /// Get the spendable balance of "id". This will return the same value as balance()
    /// unless this is called on the Stellar Asset Contract, in which case this can
    /// be less due to reserves/liabilities.
    fn spendable(env: Env, id: Address) -> i128;

    // Returns true if "id" is authorized to use its balance.
    fn authorized(env: Env, id: Address) -> bool;

    /// Get the allowance for "spender" to transfer from "from".
    fn allowance(env: Env, from: Address, spender: Address) -> i128;

    // --------------------------------------------------------------------------------
    // Descriptive Interface
    // --------------------------------------------------------------------------------

    // Get the number of decimals used to represent amounts of this token.
    fn decimals(env: Env) -> u32;

    // Get the name for this token.
    fn name(env: Env) -> Bytes;

    // Get the symbol for this token.
    fn symbol(env: Env) -> Bytes;
}

/// An interface exposing the Pool information for a Blend B_Token or D_Token
#[contractclient(name = "BlendPoolTokenClient")]
pub trait BlendPoolToken {
    /// The address of the pool the token belongs too.
    fn pool(env: Env) -> Address;

    /// The asset the token represents in the pool.
    fn asset(env: Env) -> Asset;

    /// Initialize the Blend Pool Token with an asset where "asset" represents
    /// the address of the underlying asset and "index" is the asset's reserve index
    /// in the pool.
    ///
    /// Can only be set once.
    // @dev: Remove pool arg once Address <-> BytesN conversions are added
    fn init_asset(env: Env, admin: Address, pool: BytesN<32>, asset: BytesN<32>, index: u32);
}
