use soroban_sdk::{symbol, Address, Env};

pub(crate) fn incr_allow(e: &Env, from: Address, to: Address, amount: i128) {
    let topics = (symbol!("incr_allow"), from, to);
    e.events().publish(topics, amount);
}

pub(crate) fn decr_allow(e: &Env, from: Address, to: Address, amount: i128) {
    let topics = (symbol!("decr_allow"), from, to);
    e.events().publish(topics, amount);
}

pub(crate) fn transfer(e: &Env, from: Address, to: Address, amount: i128) {
    let topics = (symbol!("transfer"), from, to);
    e.events().publish(topics, amount);
}

pub(crate) fn mint(e: &Env, admin: Address, to: Address, amount: i128) {
    let topics = (symbol!("mint"), admin, to);
    e.events().publish(topics, amount);
}

pub(crate) fn clawback(e: &Env, admin: Address, from: Address, amount: i128) {
    let topics = (symbol!("clawback"), admin, from);
    e.events().publish(topics, amount);
}
