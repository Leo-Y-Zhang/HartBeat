//! What can go wrong, and the addresses it went wrong at.

use core::fmt;

/// A synchronous exception. The hart stops; the caller decides what that means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trap {
    /// The word at `pc` is not an instruction this hart implements.
    IllegalInstruction { pc: u32, word: u32 },
    /// A load or store whose address is not naturally aligned.
    ///
    /// The base ISA permits a hart to handle these in hardware; this one refuses
    /// them, which is the stricter of the two allowed behaviours and is what the
    /// reference implementation does too. Any disagreement here would be a
    /// disagreement about the *choice*, so both had to make the same one, and
    /// this comment is where that is admitted.
    MisalignedAccess { pc: u32, addr: u32, width: u32 },
    /// A load or store to an address no memory region covers.
    AccessFault { pc: u32, addr: u32 },
    /// `ecall`.
    EnvironmentCall { pc: u32 },
    /// `ebreak`.
    Breakpoint { pc: u32 },
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trap::IllegalInstruction { pc, word } => {
                write!(f, "illegal instruction {word:#010x} at pc {pc:#010x}")
            }
            Trap::MisalignedAccess { pc, addr, width } => write!(
                f,
                "misaligned {width}-byte access to {addr:#010x} at pc {pc:#010x}"
            ),
            Trap::AccessFault { pc, addr } => {
                write!(f, "access fault at {addr:#010x} from pc {pc:#010x}")
            }
            Trap::EnvironmentCall { pc } => write!(f, "ecall at pc {pc:#010x}"),
            Trap::Breakpoint { pc } => write!(f, "ebreak at pc {pc:#010x}"),
        }
    }
}
