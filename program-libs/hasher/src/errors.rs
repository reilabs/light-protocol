use light_poseidon::PoseidonError;
use thiserror::Error;

use crate::poseidon::PoseidonSyscallError;

#[derive(Debug, Error, PartialEq)]
pub enum HasherError {
    #[error("Integer overflow, value too large")]
    IntegerOverflow,
    #[error("Poseidon hasher error: {0}")]
    Poseidon(#[from] PoseidonError),
    #[error("Poseidon syscall error: {0}")]
    PoseidonSyscall(#[from] PoseidonSyscallError),
    #[error("Unknown Solana syscall error: {0}")]
    UnknownSolanaSyscall(u64),
    #[error("Poseidon hash inputs must be 32 bytes {0}")]
    InvalidInputLength(usize),
}

// NOTE(vadorovsky): Unfortunately, we need to do it by hand. `num_derive::ToPrimitive`
// doesn't support data-carrying enums.
impl From<HasherError> for u32 {
    fn from(e: HasherError) -> u32 {
        match e {
            HasherError::IntegerOverflow => 7001,
            HasherError::Poseidon(_) => 7002,
            HasherError::PoseidonSyscall(e) => (u64::from(e)).try_into().unwrap_or(7003),
            HasherError::UnknownSolanaSyscall(e) => e.try_into().unwrap_or(7004),
            HasherError::InvalidInputLength(_) => 7005,
        }
    }
}

impl From<HasherError> for solana_program::program_error::ProgramError {
    fn from(e: HasherError) -> Self {
        solana_program::program_error::ProgramError::Custom(e.into())
    }
}
