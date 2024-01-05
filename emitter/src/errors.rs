use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
/// Error codes for the emitter contract. Common errors are codes that match up with the built-in
/// contracts error reporting. Emitter specific errors start at 1100.
pub enum EmitterError {
    // Common Errors
    InternalError = 1,
    AlreadyInitializedError = 3,

    UnauthorizedError = 4,

    // Backstop
    InsufficientBackstopSize = 1100,
    BadDrop = 1101,
    SwapNotQueued = 1102,
    SwapAlreadyExists = 1103,
    SwapNotUnlocked = 1104,
    SwapCannotBeCanceled = 1105,
}
