use soroban_auth::Identifier;
use soroban_sdk::contracttype;

#[contracttype]
pub struct AllowanceDataKey {
    pub from: Identifier,
    pub spender: Identifier,
}

#[contracttype]
pub enum DataKey {
    Balance(Identifier),
    Nonce(Identifier),
    State(Identifier),
    Admin,
    Metadata,
}
