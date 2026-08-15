//! Instruction decoding: a 32-bit word to a typed instruction.
//!
//! This half of the emulator exists as a separate step on purpose. The reference
//! implementation in `hartbeat-ref` has no decode step at all -- it pulls fields
//! straight out of the word at the point of use -- so a mistake in the shape of
//! this enum cannot be mirrored there and cancel out in the lockstep comparison.

/// Register-register and register-immediate arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp {
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
}

/// The M extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MulOp {
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchOp {
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOp {
    B,
    H,
    W,
    Bu,
    Hu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOp {
    B,
    H,
    W,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    Lui {
        rd: u8,
        imm: u32,
    },
    Auipc {
        rd: u8,
        imm: u32,
    },
    Jal {
        rd: u8,
        imm: i32,
    },
    Jalr {
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Branch {
        op: BranchOp,
        rs1: u8,
        rs2: u8,
        imm: i32,
    },
    Load {
        op: LoadOp,
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Store {
        op: StoreOp,
        rs1: u8,
        rs2: u8,
        imm: i32,
    },
    /// Shifts arrive here with `imm` holding the shift amount.
    OpImm {
        op: AluOp,
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Op {
        op: AluOp,
        rd: u8,
        rs1: u8,
        rs2: u8,
    },
    Mul {
        op: MulOp,
        rd: u8,
        rs1: u8,
        rs2: u8,
    },
    Fence,
    Ecall,
    Ebreak,
}

/// Sign-extend the low `bits` bits of `value`.
#[inline]
fn sext(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

#[inline]
fn rd(word: u32) -> u8 {
    ((word >> 7) & 0x1f) as u8
}
#[inline]
fn rs1(word: u32) -> u8 {
    ((word >> 15) & 0x1f) as u8
}
#[inline]
fn rs2(word: u32) -> u8 {
    ((word >> 20) & 0x1f) as u8
}
#[inline]
fn funct3(word: u32) -> u32 {
    (word >> 12) & 0x7
}
#[inline]
fn funct7(word: u32) -> u32 {
    word >> 25
}

#[inline]
fn imm_i(word: u32) -> i32 {
    sext(word >> 20, 12)
}

#[inline]
fn imm_s(word: u32) -> i32 {
    sext(((word >> 25) << 5) | ((word >> 7) & 0x1f), 12)
}

/// `imm[12|10:5]` from 31..25 and `imm[4:1|11]` from 11..7, with bit 0 zero.
#[inline]
fn imm_b(word: u32) -> i32 {
    let bits = (((word >> 31) & 1) << 12)
        | (((word >> 7) & 1) << 11)
        | (((word >> 25) & 0x3f) << 5)
        | (((word >> 8) & 0xf) << 1);
    sext(bits, 13)
}

/// `imm[20|10:1|11|19:12]` from 31..12, with bit 0 zero.
#[inline]
fn imm_j(word: u32) -> i32 {
    let bits = (((word >> 31) & 1) << 20)
        | (((word >> 12) & 0xff) << 12)
        | (((word >> 20) & 1) << 11)
        | (((word >> 21) & 0x3ff) << 1);
    sext(bits, 21)
}

/// Decode one word. `None` means no RV32IM instruction has this encoding.
///
/// Anything not in RV32IM is rejected rather than approximated. That includes
/// the CSR instructions and `fence.i`, which belong to extensions this hart does
/// not implement: accepting them as no-ops would let a program that depends on
/// them appear to work.
pub fn decode(word: u32) -> Option<Instr> {
    let opcode = word & 0x7f;
    match opcode {
        0b0110111 => Some(Instr::Lui {
            rd: rd(word),
            imm: word & 0xffff_f000,
        }),
        0b0010111 => Some(Instr::Auipc {
            rd: rd(word),
            imm: word & 0xffff_f000,
        }),
        0b1101111 => Some(Instr::Jal {
            rd: rd(word),
            imm: imm_j(word),
        }),
        0b1100111 if funct3(word) == 0 => Some(Instr::Jalr {
            rd: rd(word),
            rs1: rs1(word),
            imm: imm_i(word),
        }),
        0b1100011 => {
            let op = match funct3(word) {
                0b000 => BranchOp::Eq,
                0b001 => BranchOp::Ne,
                0b100 => BranchOp::Lt,
                0b101 => BranchOp::Ge,
                0b110 => BranchOp::Ltu,
                0b111 => BranchOp::Geu,
                _ => return None,
            };
            Some(Instr::Branch {
                op,
                rs1: rs1(word),
                rs2: rs2(word),
                imm: imm_b(word),
            })
        }
        0b0000011 => {
            let op = match funct3(word) {
                0b000 => LoadOp::B,
                0b001 => LoadOp::H,
                0b010 => LoadOp::W,
                0b100 => LoadOp::Bu,
                0b101 => LoadOp::Hu,
                _ => return None,
            };
            Some(Instr::Load {
                op,
                rd: rd(word),
                rs1: rs1(word),
                imm: imm_i(word),
            })
        }
        0b0100011 => {
            let op = match funct3(word) {
                0b000 => StoreOp::B,
                0b001 => StoreOp::H,
                0b010 => StoreOp::W,
                _ => return None,
            };
            Some(Instr::Store {
                op,
                rs1: rs1(word),
                rs2: rs2(word),
                imm: imm_s(word),
            })
        }
        0b0010011 => {
            let (op, imm) = match funct3(word) {
                0b000 => (AluOp::Add, imm_i(word)),
                0b010 => (AluOp::Slt, imm_i(word)),
                0b011 => (AluOp::Sltu, imm_i(word)),
                0b100 => (AluOp::Xor, imm_i(word)),
                0b110 => (AluOp::Or, imm_i(word)),
                0b111 => (AluOp::And, imm_i(word)),
                // The shift amount is five bits on RV32; the seven bits above it
                // select the variant and nothing else is legal.
                0b001 if funct7(word) == 0b0000000 => (AluOp::Sll, (rs2(word) as i32)),
                0b101 if funct7(word) == 0b0000000 => (AluOp::Srl, (rs2(word) as i32)),
                0b101 if funct7(word) == 0b0100000 => (AluOp::Sra, (rs2(word) as i32)),
                _ => return None,
            };
            Some(Instr::OpImm {
                op,
                rd: rd(word),
                rs1: rs1(word),
                imm,
            })
        }
        0b0110011 => match funct7(word) {
            0b0000000 => {
                let op = match funct3(word) {
                    0b000 => AluOp::Add,
                    0b001 => AluOp::Sll,
                    0b010 => AluOp::Slt,
                    0b011 => AluOp::Sltu,
                    0b100 => AluOp::Xor,
                    0b101 => AluOp::Srl,
                    0b110 => AluOp::Or,
                    0b111 => AluOp::And,
                    _ => return None,
                };
                Some(Instr::Op {
                    op,
                    rd: rd(word),
                    rs1: rs1(word),
                    rs2: rs2(word),
                })
            }
            0b0100000 => {
                let op = match funct3(word) {
                    0b000 => AluOp::Sub,
                    0b101 => AluOp::Sra,
                    _ => return None,
                };
                Some(Instr::Op {
                    op,
                    rd: rd(word),
                    rs1: rs1(word),
                    rs2: rs2(word),
                })
            }
            0b0000001 => {
                let op = match funct3(word) {
                    0b000 => MulOp::Mul,
                    0b001 => MulOp::Mulh,
                    0b010 => MulOp::Mulhsu,
                    0b011 => MulOp::Mulhu,
                    0b100 => MulOp::Div,
                    0b101 => MulOp::Divu,
                    0b110 => MulOp::Rem,
                    0b111 => MulOp::Remu,
                    _ => return None,
                };
                Some(Instr::Mul {
                    op,
                    rd: rd(word),
                    rs1: rs1(word),
                    rs2: rs2(word),
                })
            }
            _ => None,
        },
        0b0001111 if funct3(word) == 0b000 => Some(Instr::Fence),
        0b1110011 if funct3(word) == 0b000 => match word >> 20 {
            0 => Some(Instr::Ecall),
            1 => Some(Instr::Ebreak),
            _ => None,
        },
        _ => None,
    }
}
