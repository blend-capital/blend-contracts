pub fn to_shares(total_shares: u64, total_tokens: u64, amount: u64) -> u64 {
    if total_shares == 0 {
        return amount;
    }

    (amount * total_shares) / total_tokens
}

pub fn to_tokens(total_shares: u64, total_tokens: u64, amount: u64) -> u64 {
    if total_tokens == 0 {
        return amount;
    }

    (amount * total_tokens) / total_shares
}
