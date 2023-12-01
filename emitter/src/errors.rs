use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EmitterError {
    AlreadyInitialized = 10,
    NotAuthorized = 20,
    InsufficientBackstopSize = 30,
    BadDrop = 40,
    SwapNotQueued = 50,
    SwapAlreadyExists = 60,
    SwapNotUnlocked = 70,
    SwapCannotBeCanceled = 80,
}
