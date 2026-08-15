//! Instruction semantics.
//!
//! `pc` is advanced here rather than by the caller, because jumps and branches
//! need to replace it rather than adjust it, and a design where the caller adds
//! four afterwards has to special-case exactly the instructions most likely to
//! be wrong.

use crate::decode::{AluOp, BranchOp, Instr, LoadOp, MulOp, StoreOp};
use crate::hart::Hart;
use crate::trap::Trap;

/// Shift amounts use the low five bits of the operand on RV32. Everything else
/// here is the obvious operation on 32-bit words.
fn alu(op: AluOp, a: u32, b: u32) -> u32 {
    match op {
        AluOp::Add => a.wrapping_add(b),
        AluOp::Sub => a.wrapping_sub(b),
        AluOp::Sll => a << (b & 0x1f),
        AluOp::Slt => ((a as i32) < (b as i32)) as u32,
        AluOp::Sltu => (a < b) as u32,
        AluOp::Xor => a ^ b,
        AluOp::Srl => a >> (b & 0x1f),
        AluOp::Sra => ((a as i32) >> (b & 0x1f)) as u32,
        AluOp::Or => a | b,
        AluOp::And => a & b,
    }
}

/// The M extension.
///
/// Division has no trapping case: dividing by zero and the one signed overflow
/// both have specified results, and both differ from what the host would do.
/// `wrapping_div` is used for the overflow case rather than relying on it,
/// because `i32::MIN / -1` panics in Rust.
///
/// Clippy suggests folding the zero checks into `checked_div`. That is declined
/// deliberately: `checked_div` says "there is no answer", and the whole point
/// here is that RISC-V specifies an answer. The explicit branches are the
/// specification, written out.
#[allow(clippy::manual_checked_ops)]
fn muldiv(op: MulOp, a: u32, b: u32) -> u32 {
    const MIN: u32 = 0x8000_0000;
    const NEG_ONE: u32 = 0xffff_ffff;
    match op {
        MulOp::Mul => a.wrapping_mul(b),
        MulOp::Mulh => (((a as i32 as i64) * (b as i32 as i64)) >> 32) as u32,
        MulOp::Mulhsu => (((a as i32 as i64) * (b as i64)) >> 32) as u32,
        MulOp::Mulhu => (((a as u64) * (b as u64)) >> 32) as u32,
        MulOp::Div => {
            if b == 0 {
                NEG_ONE
            } else if a == MIN && b == NEG_ONE {
                MIN
            } else {
                ((a as i32).wrapping_div(b as i32)) as u32
            }
        }
        MulOp::Divu => {
            if b == 0 {
                NEG_ONE
            } else {
                a / b
            }
        }
        MulOp::Rem => {
            if b == 0 {
                a
            } else if a == MIN && b == NEG_ONE {
                0
            } else {
                ((a as i32).wrapping_rem(b as i32)) as u32
            }
        }
        MulOp::Remu => {
            if b == 0 {
                a
            } else {
                a % b
            }
        }
    }
}

fn branch_taken(op: BranchOp, a: u32, b: u32) -> bool {
    match op {
        BranchOp::Eq => a == b,
        BranchOp::Ne => a != b,
        BranchOp::Lt => (a as i32) < (b as i32),
        BranchOp::Ge => (a as i32) >= (b as i32),
        BranchOp::Ltu => a < b,
        BranchOp::Geu => a >= b,
    }
}

/// Execute one decoded instruction, including advancing `pc`.
pub fn execute(hart: &mut Hart, instr: Instr) -> Result<(), Trap> {
    let pc = hart.pc;
    let next = pc.wrapping_add(4);

    match instr {
        Instr::Lui { rd, imm } => {
            hart.set_reg(rd, imm);
            hart.pc = next;
        }
        Instr::Auipc { rd, imm } => {
            hart.set_reg(rd, pc.wrapping_add(imm));
            hart.pc = next;
        }
        Instr::Jal { rd, imm } => {
            hart.set_reg(rd, next);
            hart.pc = pc.wrapping_add(imm as u32);
        }
        Instr::Jalr { rd, rs1, imm } => {
            // The target is computed before rd is written, because they can be
            // the same register.
            let target = hart.reg(rs1).wrapping_add(imm as u32) & !1;
            hart.set_reg(rd, next);
            hart.pc = target;
        }
        Instr::Branch { op, rs1, rs2, imm } => {
            hart.pc = if branch_taken(op, hart.reg(rs1), hart.reg(rs2)) {
                pc.wrapping_add(imm as u32)
            } else {
                next
            };
        }
        Instr::Load { op, rd, rs1, imm } => {
            let addr = hart.reg(rs1).wrapping_add(imm as u32);
            let value = match op {
                LoadOp::B => {
                    let raw = hart.mem.read(addr, 1, pc)?;
                    ((raw as u8) as i8) as i32 as u32
                }
                LoadOp::H => {
                    let raw = hart.mem.read(addr, 2, pc)?;
                    ((raw as u16) as i16) as i32 as u32
                }
                LoadOp::W => hart.mem.read(addr, 4, pc)?,
                LoadOp::Bu => hart.mem.read(addr, 1, pc)?,
                LoadOp::Hu => hart.mem.read(addr, 2, pc)?,
            };
            hart.set_reg(rd, value);
            hart.pc = next;
        }
        Instr::Store { op, rs1, rs2, imm } => {
            let addr = hart.reg(rs1).wrapping_add(imm as u32);
            let value = hart.reg(rs2);
            let width = match op {
                StoreOp::B => 1,
                StoreOp::H => 2,
                StoreOp::W => 4,
            };
            hart.mem.write(addr, value, width, pc)?;
            hart.pc = next;
        }
        Instr::OpImm { op, rd, rs1, imm } => {
            hart.set_reg(rd, alu(op, hart.reg(rs1), imm as u32));
            hart.pc = next;
        }
        Instr::Op { op, rd, rs1, rs2 } => {
            hart.set_reg(rd, alu(op, hart.reg(rs1), hart.reg(rs2)));
            hart.pc = next;
        }
        Instr::Mul { op, rd, rs1, rs2 } => {
            hart.set_reg(rd, muldiv(op, hart.reg(rs1), hart.reg(rs2)));
            hart.pc = next;
        }
        Instr::Fence => {
            // One hart, no store buffer, nothing to order.
            hart.pc = next;
        }
        Instr::Ecall => return Err(Trap::EnvironmentCall { pc }),
        Instr::Ebreak => return Err(Trap::Breakpoint { pc }),
    }
    Ok(())
}
