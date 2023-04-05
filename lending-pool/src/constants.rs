// The address of the Blend Pool Factory
pub const POOL_FACTORY: [u8; 32] = [101; 32]; // TODO: Use the actual pool factory address

// The emitter contract
pub const EMITTER: [u8; 32] = [100; 32];

/// The address of the BLND token
pub const BLND_TOKEN: [u8; 32] = [222; 32]; // TODO: Use actual token bytes

/// The address of the USDC token
pub const USDC_TOKEN: [u8; 32] = [233; 32]; // TODO: Use actual token bytes

/********** Numbers **********/

/// Fixed-point scalar for 9 decimal numbers
pub const SCALAR_9: i128 = 1_000_000_000;

/// Fixed-point scalar for 7 decimal numbers
pub const SCALAR_7: i128 = 1_0000000;

/// Average number of blocks per year based on 5 second finality
pub const BLOCKS_PER_YEAR: i128 = 6307200;
