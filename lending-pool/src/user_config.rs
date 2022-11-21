/**
 *  Adapted from: https://github.com/aave/aave-v3-core/blob/master/contracts/protocol/libraries/configuration/UserConfiguration.sol
 */

pub trait UserConfigurator {
    /// Determine is a reserve is being used by a user, where used means
    /// the asset is used as collateral or borrowed by the user
    /// 
    /// ### Arguments
    /// * `user_config` - The current user configuration
    /// * `res_index` - The index of the reserve to check
    fn is_using_reserve(&self, res_index: u32) -> bool;

    /// Checks if the user is borrowing a reserve
    /// 
    /// ### Arguments
    /// * `res_index` - The index of the reserve to check
    fn is_borrowing(&self, res_index: u32) -> bool;
    
    /// Checks if the user is using the reserve as collateral
    ///
    /// ### Arguments
    /// * `res_index` - The index of the reserve to check
    fn is_collateral(&self, res_index: u32) -> bool;
    
    /// Set the user config based on the new borrowing status of the reserve at the res_index
    /// 
    /// ### Arguments
    /// * `res_index` - The index of the reserve
    /// * `borrowing` - If the user is borrowing the reserve
    fn set_borrowing(&mut self, res_index: u32, borrowing: bool);
    
    /// Set the user config based on the new collateral status of the reserve at the res_index
    /// 
    /// ### Arguments
    /// * `res_index` - The index of the reserve
    /// * `collateral` - If the user is using the reserve as collateral
    fn set_collateral(&mut self, res_index: u32, collateral: bool);
}

pub struct UserConfig {
    pub config: u64,
}

impl UserConfig {
    pub fn new(user_config: u64) -> UserConfig {
        UserConfig { config: user_config }
    }
}

impl UserConfigurator for UserConfig {
    fn is_using_reserve(&self, res_index: u32) -> bool {
        let to_res_shift = res_index * 2;
        (self.config >> to_res_shift) & 0b11 != 0
    }
    
    fn is_borrowing(&self, res_index: u32) -> bool {
        let to_res_shift = res_index * 2;
        (self.config >> to_res_shift) & 0b01 != 0
    }
    
    fn is_collateral(&self, res_index: u32) -> bool {
        let to_res_shift = res_index * 2;
        (self.config >> to_res_shift) & 0b10 != 0
    }
    
    fn set_borrowing(&mut self, res_index: u32, borrowing: bool) {
        let res_borrow_bit = 1 << (res_index * 2);
        if borrowing {
            // set bit to 1
            self.config = self.config | res_borrow_bit;
        } else {
            // set bit to zero
            self.config = self.config & !res_borrow_bit;
        }
    }
    
    fn set_collateral(&mut self, res_index: u32, collateral: bool) {
        let res_collateral_bit = 1 << (res_index * 2 + 1);
        if collateral {
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
        let user_config: UserConfig = UserConfig::new(0xFFFFFFFFFFFFFCFF);
        let res_index = 4;

        let is_using = user_config.is_using_reserve(res_index);
        let is_collateral = user_config.is_collateral(res_index);
        let is_borrowing =  user_config.is_borrowing(res_index);

        assert_eq!(is_using, false);
        assert_eq!(is_collateral, false);
        assert_eq!(is_borrowing, false);
    }

    #[test]
    fn test_user_config_using_all() {
        let user_config: UserConfig = UserConfig::new(0x0003000000000000);
        let res_index = 24;

        let is_using = user_config.is_using_reserve(res_index);
        let is_collateral = user_config.is_collateral(res_index);
        let is_borrowing =  user_config.is_borrowing(res_index);

        assert_eq!(is_using, true);
        assert_eq!(is_collateral, true);
        assert_eq!(is_borrowing, true);
    }

    #[test]
    fn test_user_config_only_collateral() {
        let user_config: UserConfig = UserConfig::new(0x8000000000000000);
        let res_index = 31;

        let is_using = user_config.is_using_reserve(res_index);
        let is_collateral = user_config.is_collateral(res_index);
        let is_borrowing =  user_config.is_borrowing(res_index);

        assert_eq!(is_using, true);
        assert_eq!(is_collateral, true);
        assert_eq!(is_borrowing, false);
    }

    #[test]
    fn test_user_config_only_borrowing() {
        let user_config: UserConfig = UserConfig::new(0x0000000000000001);
        let res_index = 0;

        let is_using = user_config.is_using_reserve(res_index);
        let is_collateral = user_config.is_collateral(res_index);
        let is_borrowing =  user_config.is_borrowing(res_index);

        assert_eq!(is_using, true);
        assert_eq!(is_collateral, false);
        assert_eq!(is_borrowing, true);
    }

    #[test]
    fn test_set_borrowing() {
        let mut user_config: UserConfig = UserConfig::new(thread_rng().next_u64());
        let res_index: u32 = thread_rng().next_u32() % 32;

        // setup - reset the bit 
        user_config.set_borrowing(res_index, false);
        assert_eq!(user_config.is_borrowing(res_index), false);

        // set the bit
        user_config.set_borrowing(res_index, true);
        assert_eq!(user_config.is_borrowing(res_index), true);

        // set the bit again (not toggling)
        user_config.set_borrowing(res_index, true);
        assert_eq!(user_config.is_borrowing(res_index), true);

        // reset the bit 
        user_config.set_borrowing(res_index, false);
        assert_eq!(user_config.is_borrowing(res_index), false);

        // reset the bit again (not toggling)
        user_config.set_borrowing(res_index, false);
        assert_eq!(user_config.is_borrowing(res_index), false);
    }

    #[test]
    fn test_set_collateral() {
        let mut user_config: UserConfig = UserConfig::new(thread_rng().next_u64());
        let res_index: u32 = thread_rng().next_u32() % 32;

        // setup - reset the bit 
        user_config.set_collateral(res_index, false);
        assert_eq!(user_config.is_collateral(res_index), false);

        // set the bit
        user_config.set_collateral(res_index, true);
        assert_eq!(user_config.is_collateral(res_index), true);

        // set the bit again (not toggling)
        user_config.set_collateral(res_index, true);
        assert_eq!(user_config.is_collateral(res_index), true);

        // reset the bit 
        user_config.set_collateral(res_index, false);
        assert_eq!(user_config.is_collateral(res_index), false);

        // reset the bit again (not toggling)
        user_config.set_collateral(res_index, false);
        assert_eq!(user_config.is_collateral(res_index), false);
    }
}