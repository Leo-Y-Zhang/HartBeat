//! Instruction encoders and small harnesses shared by the test files.
//!
//! These encoders are written from the format diagrams in the unprivileged
//! specification. They are *not* independent of the decoder in the sense that
//! matters most -- both were written by the same person from the same document,
//! so a shared misreading of, say, the B-type immediate layout would cancel out
//! and both would agree on a wrong answer.
//!
//! That gap is why `tests/programs.rs` exists: the instruction words there were
//! encoded by LLVM, which read the specification independently.

#![allow(dead_code)]

use hartbeat::{Hart, Stop};

pub const ENTRY: u32 = 0x8000_0000;
pub const DATA: u32 = 0x8001_0000;

pub fn r_type(funct7: u32, rs2: u32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

pub fn i_type(imm: i32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
    let imm = (imm as u32) & 0xfff;
    (imm << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

pub fn s_type(imm: i32, rs2: u32, rs1: u32, funct3: u32, opcode: u32) -> u32 {
    let imm = (imm as u32) & 0xfff;
    let hi = (imm >> 5) & 0x7f;
    let lo = imm & 0x1f;
    (hi << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (lo << 7) | opcode
}

/// B-type: `imm[12|10:5]` in 31..25 and `imm[4:1|11]` in 11..7. Bit 0 is always
/// zero and is not encoded.
pub fn b_type(imm: i32, rs2: u32, rs1: u32, funct3: u32, opcode: u32) -> u32 {
    let imm = imm as u32;
    let bit12 = (imm >> 12) & 1;
    let bits10_5 = (imm >> 5) & 0x3f;
    let bits4_1 = (imm >> 1) & 0xf;
    let bit11 = (imm >> 11) & 1;
    (bit12 << 31)
        | (bits10_5 << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (funct3 << 12)
        | (bits4_1 << 8)
        | (bit11 << 7)
        | opcode
}

pub fn u_type(imm: u32, rd: u32, opcode: u32) -> u32 {
    (imm & 0xffff_f000) | (rd << 7) | opcode
}

/// J-type: `imm[20|10:1|11|19:12]` in 31..12.
pub fn j_type(imm: i32, rd: u32, opcode: u32) -> u32 {
    let imm = imm as u32;
    let bit20 = (imm >> 20) & 1;
    let bits10_1 = (imm >> 1) & 0x3ff;
    let bit11 = (imm >> 11) & 1;
    let bits19_12 = (imm >> 12) & 0xff;
    (bit20 << 31) | (bits10_1 << 21) | (bit11 << 20) | (bits19_12 << 12) | (rd << 7) | opcode
}

// Opcodes.
pub const OP_LUI: u32 = 0b0110111;
pub const OP_AUIPC: u32 = 0b0010111;
pub const OP_JAL: u32 = 0b1101111;
pub const OP_JALR: u32 = 0b1100111;
pub const OP_BRANCH: u32 = 0b1100011;
pub const OP_LOAD: u32 = 0b0000011;
pub const OP_STORE: u32 = 0b0100011;
pub const OP_IMM: u32 = 0b0010011;
pub const OP_REG: u32 = 0b0110011;
pub const OP_SYSTEM: u32 = 0b1110011;
pub const OP_FENCE: u32 = 0b0001111;

/// A hart with the given program at `ENTRY`, one page of data at `DATA`, and the
/// given registers preset.
pub fn hart_with(program: &[u32], setup: &[(u8, u32)]) -> Hart {
    let mut hart = Hart::new(ENTRY);
    let mut bytes = Vec::with_capacity(program.len() * 4);
    for word in program {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    hart.mem.write_slice(ENTRY, &bytes);
    // Make the data page exist so that a store there is not an access fault.
    hart.mem.write_slice(DATA, &[0u8; 256]);
    for (reg, value) in setup {
        hart.set_reg(*reg, *value);
    }
    hart
}

/// Run one instruction and return the resulting state.
pub fn step_one(program: &[u32], setup: &[(u8, u32)]) -> Hart {
    let mut hart = hart_with(program, setup);
    hart.step().expect("instruction should not trap");
    hart
}

/// Run until it stops, with a budget.
pub fn run(program: &[u32], setup: &[(u8, u32)], budget: u64) -> (Hart, Stop) {
    let mut hart = hart_with(program, setup);
    let stop = hart.run(budget);
    (hart, stop)
}
