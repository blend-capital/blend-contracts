use soroban_sdk::{contracttype, Bytes};

#[derive(Clone)]
#[contracttype]
pub struct TokenMetadata {
    pub name: Bytes,
    pub symbol: Bytes,
    pub decimals: u32,
}

#[derive(Clone)]
#[contracttype]
pub enum Metadata {
    Token(TokenMetadata),
}
