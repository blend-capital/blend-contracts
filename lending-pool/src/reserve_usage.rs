// Adapted from: https://github.com/aave/aave-v3-core/blob/master/contracts/protocol/libraries/configuration/UserConfiguration.sol

/// Packs a `u128` to represent the configuration of what reserves are used and how:
///
/// * `liability` - the reserve d_token is used
/// * `supply` - the reserve b_token is used
/// * `collateral` - the reserve b_token is used as collateral
///
/// The u128 is packed from LSB to MSB for each reserve such that ->
///
/// LSB -> 0 / 1 = (liability flag) not used as liability / used as liability\
/// -----> 0 / 1 = (supply flag) not used as supply / used as supply\
/// MSB -> 0 / 1 = (disable collateral flag) collateral active / collateral disabled\
///
/// Supports a maximum of 42 indexable reserves. The final 2 bits are unused.
pub struct ReserveUsage {
    pub config: u128,
}

impl ReserveUsage {
    pub fn new(user_config: u128) -> ReserveUsage {
        ReserveUsage {
            config: user_config,
        }
    }

    /// Fetch the key for the reserve liability token
    pub fn liability_key(res_index: u32) -> u32 {
        res_index * 3
    }

    /// Fetch the key for the reserve supply token
    pub fn supply_key(res_index: u32) -> u32 {
        res_index * 3 + 1
    }

    /// Fetch the key for the reserve collateral disabled flag
    pub fn collateral_disabled_key(res_index: u32) -> u32 {
        res_index * 3 + 2
    }

    /// Determine if a reserve is being actively used, where active means the reserve is used as
    /// a liability or as collateral
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve to check
    pub fn is_active_reserve(&self, res_index: u32) -> bool {
        let res_config = self.config >> (res_index * 3);
        res_config & 0b1 != 0 || res_config & 0b110 == 0b010
    }

    /// Checks if the reserve liability is being used
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve to check
    pub fn is_liability(&self, res_index: u32) -> bool {
        let to_res_shift = res_index * 3;
        (self.config >> to_res_shift) & 0b1 != 0
    }

    /// Checks if the reserve supply is being used
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve to check
    pub fn is_supply(&self, res_index: u32) -> bool {
        let to_res_shift = res_index * 3;
        (self.config >> to_res_shift) & 0b10 != 0
    }

    /// Checks if the reserve supply is being used as collateral
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve to check
    pub fn is_collateral(&self, res_index: u32) -> bool {
        let to_res_shift = res_index * 3;
        (self.config >> to_res_shift) & 0b110 == 0b010
    }

    /// Checks if the reserve disable collateral flag is active
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve to check
    pub fn is_collateral_disabled(&self, res_index: u32) -> bool {
        let to_res_shift = res_index * 3;
        (self.config >> to_res_shift) & 0b100 != 0
    }

    /// Set the user config based on the new borrowing status of the reserve at the res_index
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve
    /// * `borrowing` - If the user is borrowing the reserve
    pub fn set_liability(&mut self, res_index: u32, borrowing: bool) {
        let res_borrow_bit = 1 << (Self::liability_key(res_index));
        if borrowing {
            // set bit to 1
            self.config = self.config | res_borrow_bit;
        } else {
            // set bit to zero
            self.config = self.config & !res_borrow_bit;
        }
    }

    /// Set the user config based on the new collateral status of the reserve at the res_index
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve
    /// * `collateral` - If the user is using the reserve as collateral
    pub fn set_supply(&mut self, res_index: u32, supply: bool) {
        let res_supply_bit = 1 << (Self::supply_key(res_index));
        if supply {
            // set bit to 1
            self.config = self.config | res_supply_bit;
        } else {
            // set bit to zero
            self.config = self.config & !res_supply_bit;
        }
    }

    /// Set the user config based on the new collateral disabled status of the reserve at the res_index
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve
    /// * `collateral_disabled` - If collateral usage is disabled
    pub fn set_collateral_disabled(&mut self, res_index: u32, collateral_disabled: bool) {
        let res_collateral_bit = 1 << (Self::collateral_disabled_key(res_index));
        if collateral_disabled {
            // set bit to 1
            self.config = self.config | res_collateral_bit;
        } else {
            // set bit to zero
            self.config = self.config & !res_collateral_bit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{thread_rng, RngCore};

    #[test]
    fn test_user_config_not_using() {
        let user_config: ReserveUsage = ReserveUsage::new(0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFF1FF);
        let res_index = 3;

        let is_using = user_config.is_active_reserve(res_index);
        let is_collateral_disabled = user_config.is_collateral_disabled(res_index);
        let is_borrowing = user_config.is_liability(res_index);
        let is_supply = user_config.is_supply(res_index);
        let is_collateral = user_config.is_collateral(res_index);

        assert_eq!(is_using, false);
        assert_eq!(is_collateral_disabled, false);
        assert_eq!(is_borrowing, false);
        assert_eq!(is_supply, false);
        assert_eq!(is_collateral, false);
    }

    #[test]
    fn test_user_config_using_all() {
        let user_config: ReserveUsage = ReserveUsage::new(0x600000000000000000);
        let res_index = 23;

        let is_using = user_config.is_active_reserve(res_index);
        let is_collateral_disabled = user_config.is_collateral_disabled(res_index);
        let is_borrowing = user_config.is_liability(res_index);
        let is_supply = user_config.is_supply(res_index);
        let is_collateral = user_config.is_collateral(res_index);

        assert_eq!(is_using, true);
        assert_eq!(is_collateral_disabled, false);
        assert_eq!(is_borrowing, true);
        assert_eq!(is_supply, true);
        assert_eq!(is_collateral, true);
    }

    #[test]
    fn test_user_config_only_borrowing() {
        let user_config: ReserveUsage = ReserveUsage::new(0x1);
        let res_index = 0;

        let is_using = user_config.is_active_reserve(res_index);
        let is_collateral_disabled = user_config.is_collateral_disabled(res_index);
        let is_borrowing = user_config.is_liability(res_index);
        let is_supply = user_config.is_supply(res_index);
        let is_collateral = user_config.is_collateral(res_index);

        assert_eq!(is_using, true);
        assert_eq!(is_collateral_disabled, false);
        assert_eq!(is_borrowing, true);
        assert_eq!(is_supply, false);
        assert_eq!(is_collateral, false);
    }

    #[test]
    fn test_user_config_only_supply_collateral_enabled() {
        let user_config: ReserveUsage = ReserveUsage::new(0x100000000000000000000000C0000001);
        let res_index = 41;

        let is_using = user_config.is_active_reserve(res_index);
        let is_collateral_disabled = user_config.is_collateral_disabled(res_index);
        let is_borrowing = user_config.is_liability(res_index);
        let is_supply = user_config.is_supply(res_index);
        let is_collateral = user_config.is_collateral(res_index);

        assert_eq!(is_using, true);
        assert_eq!(is_collateral_disabled, false);
        assert_eq!(is_borrowing, false);
        assert_eq!(is_supply, true);
        assert_eq!(is_collateral, true);
    }

    #[test]
    fn test_user_config_only_supply_collateral_disabled() {
        let user_config: ReserveUsage = ReserveUsage::new(0x300000000000000000000000C0000001);
        let res_index = 41;

        let is_using = user_config.is_active_reserve(res_index);
        let is_collateral_disabled = user_config.is_collateral_disabled(res_index);
        let is_borrowing = user_config.is_liability(res_index);
        let is_supply = user_config.is_supply(res_index);
        let is_collateral = user_config.is_collateral(res_index);

        assert_eq!(is_using, false);
        assert_eq!(is_collateral_disabled, true);
        assert_eq!(is_borrowing, false);
        assert_eq!(is_supply, true);
        assert_eq!(is_collateral, false);
    }

    #[test]
    fn test_set_liability() {
        let mut user_config: ReserveUsage = ReserveUsage::new(thread_rng().next_u64() as u128);
        let res_index: u32 = thread_rng().next_u32() % 32;

        // setup - reset the bit
        user_config.set_liability(res_index, false);
        assert_eq!(user_config.is_liability(res_index), false);

        // set the bit
        user_config.set_liability(res_index, true);
        assert_eq!(user_config.is_liability(res_index), true);

        // set the bit again (not toggling)
        user_config.set_liability(res_index, true);
        assert_eq!(user_config.is_liability(res_index), true);

        // reset the bit
        user_config.set_liability(res_index, false);
        assert_eq!(user_config.is_liability(res_index), false);

        // reset the bit again (not toggling)
        user_config.set_liability(res_index, false);
        assert_eq!(user_config.is_liability(res_index), false);
    }

    #[test]
    fn test_set_supply() {
        let mut user_config: ReserveUsage = ReserveUsage::new(thread_rng().next_u64() as u128);
        let res_index: u32 = thread_rng().next_u32() % 32;

        // setup - reset the bit
        user_config.set_supply(res_index, false);
        assert_eq!(user_config.is_supply(res_index), false);

        // set the bit
        user_config.set_supply(res_index, true);
        assert_eq!(user_config.is_supply(res_index), true);

        // set the bit again (not toggling)
        user_config.set_supply(res_index, true);
        assert_eq!(user_config.is_supply(res_index), true);

        // reset the bit
        user_config.set_supply(res_index, false);
        assert_eq!(user_config.is_supply(res_index), false);

        // reset the bit again (not toggling)
        user_config.set_supply(res_index, false);
        assert_eq!(user_config.is_supply(res_index), false);
    }

    #[test]
    fn test_set_disable_collateral() {
        let mut user_config: ReserveUsage = ReserveUsage::new(thread_rng().next_u64() as u128);
        let res_index: u32 = thread_rng().next_u32() % 32;

        // setup - reset the bit
        user_config.set_collateral_disabled(res_index, false);
        assert_eq!(user_config.is_collateral_disabled(res_index), false);

        // set the bit
        user_config.set_collateral_disabled(res_index, true);
        assert_eq!(user_config.is_collateral_disabled(res_index), true);

        // set the bit again (not toggling)
        user_config.set_collateral_disabled(res_index, true);
        assert_eq!(user_config.is_collateral_disabled(res_index), true);

        // reset the bit
        user_config.set_collateral_disabled(res_index, false);
        assert_eq!(user_config.is_collateral_disabled(res_index), false);

        // reset the bit again (not toggling)
        user_config.set_collateral_disabled(res_index, false);
        assert_eq!(user_config.is_collateral_disabled(res_index), false);
    }
}
