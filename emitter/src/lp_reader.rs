// pub fn get_lp_share_value(
//     lp_token_id: &BytesN<32>,
//     lp_token_amount: &U256,
//     token_id: &Address,
//     token_amount: &U256,
// ) -> U256 {
//     let lp_token_supply = TokenClient::get_total_supply(lp_token_id);
//     let token_supply = TokenClient::get_total_supply(token_id);
//     let lp_token_share = lp_token_amount * token_supply / lp_token_supply;
//     lp_token_share * token_amount / token_supply
// }
