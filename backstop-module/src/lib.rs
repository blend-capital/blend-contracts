#![no_std]
use soroban_sdk::{contractimpl, symbol, Symbol};

pub struct Contract;

#[contractimpl]
impl Contract {
    pub fn whoami() -> Symbol {
        symbol!("backstop")
    }
}
