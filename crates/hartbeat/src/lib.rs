//! HartBeat: an RV32IM interpreter.
//!
//! The interpreter is the substrate. What the repository is actually about is
//! how it is checked: every instruction is executed twice, by this crate and by
//! `hartbeat-ref`, and the whole architectural state is compared after each one.
//! See the README for what that does and does not establish.

pub mod decode;
pub mod elf;
pub mod exec;
pub mod hart;
pub mod mem;
pub mod trap;

pub use decode::{decode, AluOp, BranchOp, Instr, LoadOp, MulOp, StoreOp};
pub use hart::{Hart, Stop};
pub use mem::Memory;
pub use trap::Trap;
