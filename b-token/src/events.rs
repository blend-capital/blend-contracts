use soroban_sdk::{Address, Env, Symbol};

pub(crate) fn incr_allow(e: &Env, from: Address, to: Address, amount: i128) {
    let topics = (Symbol::new(e, "incr_allow"), from, to);
    e.events().publish(topics, amount);
}

pub(crate) fn decr_allow(e: &Env, from: Address, to: Address, amount: i128) {
    let topics = (Symbol::new(e, "decr_allow"), from, to);
    e.events().publish(topics, amount);
}

pub(crate) fn transfer(e: &Env, from: Address, to: Address, amount: i128) {
    let topics = (Symbol::new(e, "transfer"), from, to);
    e.events().publish(topics, amount);
}

pub(crate) fn mint(e: &Env, admin: Address, to: Address, amount: i128) {
    let topics = (Symbol::new(e, "mint"), admin, to);
    e.events().publish(topics, amount);
}

pub(crate) fn clawback(e: &Env, admin: Address, from: Address, amount: i128) {
    let topics = (Symbol::new(e, "clawback"), admin, from);
    e.events().publish(topics, amount);
}
