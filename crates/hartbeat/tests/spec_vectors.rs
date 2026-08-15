//! Cases taken from the unprivileged specification, chosen because they are the
//! ones an implementation gets wrong.
//!
//! Every one of these is a place where a plausible implementation differs from
//! the specified one: a shift that should be arithmetic, a comparison that
//! should be unsigned, a division that should not trap. An emulator that runs
//! ordinary compiled code correctly can still be wrong about all of them,
//! because ordinary compiled code rarely divides by zero.

mod common;

use common::*;
use hartbeat::{Hart, Trap};

fn reg_after(program: &[u32], setup: &[(u8, u32)], reg: u8) -> u32 {
    step_one(program, setup).reg(reg)
}

// ---------------------------------------------------------------- x0

#[test]
fn x0_stays_zero_when_written() {
    // addi x0, x0, 42
    let hart = step_one(&[i_type(42, 0, 0b000, 0, OP_IMM)], &[]);
    assert_eq!(hart.reg(0), 0, "x0 must read as zero after being written");
}

// ---------------------------------------------------------------- immediates

#[test]
fn addi_sign_extends_its_immediate() {
    // addi x1, x0, -1
    assert_eq!(
        reg_after(&[i_type(-1, 0, 0b000, 1, OP_IMM)], &[], 1),
        0xffff_ffff
    );
}

#[test]
fn andi_sign_extends_before_masking() {
    // andi x1, x2, -256  ->  x2 & 0xffffff00
    let got = reg_after(&[i_type(-256, 2, 0b111, 1, OP_IMM)], &[(2, 0xdead_beef)], 1);
    assert_eq!(got, 0xdead_be00);
}

#[test]
fn sltiu_compares_the_sign_extended_immediate_as_unsigned() {
    // sltiu x1, x2, -1  ->  x2 < 0xffffffff unsigned
    let got = reg_after(&[i_type(-1, 2, 0b011, 1, OP_IMM)], &[(2, 1)], 1);
    assert_eq!(got, 1, "1 < 0xffffffff unsigned");
    let got = reg_after(&[i_type(-1, 2, 0b011, 1, OP_IMM)], &[(2, 0xffff_ffff)], 1);
    assert_eq!(got, 0, "0xffffffff is not less than itself");
}

// ---------------------------------------------------------------- shifts

/// The literal is grouped as the instruction is: seven bits of `funct7` then
/// five of shift amount, which is what makes it readable.
#[allow(clippy::unusual_byte_groupings)]
#[test]
fn srai_is_arithmetic_and_srli_is_logical() {
    let srai = i_type(0b0100000_00100, 2, 0b101, 1, OP_IMM); // srai x1, x2, 4
    let srli = i_type(4, 2, 0b101, 1, OP_IMM); // srli x1, x2, 4
    assert_eq!(reg_after(&[srai], &[(2, 0x8000_0000)], 1), 0xf800_0000);
    assert_eq!(reg_after(&[srli], &[(2, 0x8000_0000)], 1), 0x0800_0000);
}

#[test]
fn shifts_use_only_the_low_five_bits_of_the_amount() {
    // sll x1, x2, x3 with x3 = 33 must shift by 1, not by 33.
    let sll = r_type(0, 3, 2, 0b001, 1, OP_REG);
    assert_eq!(reg_after(&[sll], &[(2, 1), (3, 33)], 1), 2);
    let sra = r_type(0b0100000, 3, 2, 0b101, 1, OP_REG);
    assert_eq!(
        reg_after(&[sra], &[(2, 0x8000_0000), (3, 32 + 4)], 1),
        0xf800_0000
    );
}

// ---------------------------------------------------------------- comparisons

#[test]
fn slt_is_signed_and_sltu_is_not() {
    let slt = r_type(0, 3, 2, 0b010, 1, OP_REG);
    let sltu = r_type(0, 3, 2, 0b011, 1, OP_REG);
    // -1 < 1 signed, but 0xffffffff > 1 unsigned.
    assert_eq!(reg_after(&[slt], &[(2, 0xffff_ffff), (3, 1)], 1), 1);
    assert_eq!(reg_after(&[sltu], &[(2, 0xffff_ffff), (3, 1)], 1), 0);
}

// ---------------------------------------------------------------- jumps

#[test]
fn jal_links_the_next_instruction_and_jumps_by_a_signed_offset() {
    let hart = step_one(&[j_type(-8, 1, OP_JAL)], &[]);
    assert_eq!(hart.reg(1), ENTRY + 4, "rd gets the address after the jump");
    assert_eq!(hart.pc, ENTRY.wrapping_sub(8));
}

#[test]
fn jalr_clears_the_low_bit_of_the_target() {
    // jalr x1, x2, 3 with x2 = ENTRY + 8 -> target (ENTRY + 11) & !1
    let hart = step_one(&[i_type(3, 2, 0b000, 1, OP_JALR)], &[(2, ENTRY + 8)]);
    assert_eq!(hart.reg(1), ENTRY + 4);
    assert_eq!(hart.pc, (ENTRY + 11) & !1, "bit 0 of the target is cleared");
}

#[test]
fn jalr_computes_its_target_from_rs1_not_from_pc() {
    let hart = step_one(&[i_type(0, 2, 0b000, 0, OP_JALR)], &[(2, 0x8000_1000)]);
    assert_eq!(hart.pc, 0x8000_1000);
}

// ---------------------------------------------------------------- branches

#[test]
fn branches_take_signed_comparisons() {
    // blt x2, x3, +8
    let blt = b_type(8, 3, 2, 0b100, OP_BRANCH);
    let taken = step_one(&[blt], &[(2, 0xffff_ffff), (3, 1)]);
    assert_eq!(taken.pc, ENTRY + 8, "-1 < 1 signed, so the branch is taken");

    // bltu x2, x3, +8 on the same values must not be taken.
    let bltu = b_type(8, 3, 2, 0b110, OP_BRANCH);
    let not_taken = step_one(&[bltu], &[(2, 0xffff_ffff), (3, 1)]);
    assert_eq!(not_taken.pc, ENTRY + 4);
}

#[test]
fn branch_offsets_are_sign_extended() {
    let beq = b_type(-16, 0, 0, 0b000, OP_BRANCH); // beq x0, x0, -16
    let hart = step_one(&[beq], &[]);
    assert_eq!(hart.pc, ENTRY.wrapping_sub(16));
}

#[test]
fn bge_is_true_on_equal() {
    let bge = b_type(8, 3, 2, 0b101, OP_BRANCH);
    let hart = step_one(&[bge], &[(2, 7), (3, 7)]);
    assert_eq!(hart.pc, ENTRY + 8);
}

// ---------------------------------------------------------------- upper immediates

#[test]
fn lui_places_the_immediate_in_the_high_twenty_bits() {
    assert_eq!(
        reg_after(&[u_type(0xabcd_e000, 1, OP_LUI)], &[], 1),
        0xabcd_e000
    );
}

#[test]
fn auipc_adds_the_immediate_to_the_address_of_the_instruction() {
    assert_eq!(
        reg_after(&[u_type(0x0000_1000, 1, OP_AUIPC)], &[], 1),
        ENTRY.wrapping_add(0x1000)
    );
}

// ---------------------------------------------------------------- memory

#[test]
fn loads_sign_extend_or_zero_extend_by_width() {
    let mut hart = hart_with(&[], &[]);
    hart.mem.write_slice(DATA, &[0x80, 0x90, 0xa0, 0xb0]);

    for (funct3, expected) in [(0b000u32, 0xffff_ff80u32), (0b100, 0x0000_0080)] {
        let mut h = hart.clone();
        h.mem
            .write_slice(ENTRY, &i_type(0, 2, funct3, 1, OP_LOAD).to_le_bytes());
        h.set_reg(2, DATA);
        h.step().unwrap();
        assert_eq!(h.reg(1), expected, "funct3 {funct3:#05b}");
    }

    for (funct3, expected) in [(0b001u32, 0xffff_9080u32), (0b101, 0x0000_9080)] {
        let mut h = hart.clone();
        h.mem
            .write_slice(ENTRY, &i_type(0, 2, funct3, 1, OP_LOAD).to_le_bytes());
        h.set_reg(2, DATA);
        h.step().unwrap();
        assert_eq!(h.reg(1), expected, "funct3 {funct3:#05b}");
    }
}

#[test]
fn stores_write_only_their_width() {
    let program = [s_type(0, 3, 2, 0b000, OP_STORE)]; // sb x3, 0(x2)
    let mut hart = hart_with(&program, &[(2, DATA), (3, 0xdead_beef)]);
    hart.mem.write_slice(DATA, &[0x11, 0x22, 0x33, 0x44]);
    hart.step().unwrap();
    assert_eq!(hart.mem.read(DATA, 4, 0).unwrap(), 0x4433_22ef);
}

#[test]
fn a_misaligned_word_load_traps() {
    let program = [i_type(1, 2, 0b010, 1, OP_LOAD)]; // lw x1, 1(x2)
    let mut hart = hart_with(&program, &[(2, DATA)]);
    let err = hart.step().unwrap_err();
    assert!(matches!(err, Trap::MisalignedAccess { .. }), "got {err:?}");
}

#[test]
fn a_load_from_unmapped_memory_faults() {
    let program = [i_type(0, 2, 0b010, 1, OP_LOAD)];
    let mut hart = hart_with(&program, &[(2, 0x4000_0000)]);
    let err = hart.step().unwrap_err();
    assert!(matches!(err, Trap::AccessFault { .. }), "got {err:?}");
}

// ---------------------------------------------------------------- M extension

/// The specification fixes these; they do not trap and they do not follow the
/// host's rules for division.
#[test]
fn division_by_zero_is_defined() {
    let div = r_type(1, 3, 2, 0b100, 1, OP_REG);
    let divu = r_type(1, 3, 2, 0b101, 1, OP_REG);
    let rem = r_type(1, 3, 2, 0b110, 1, OP_REG);
    let remu = r_type(1, 3, 2, 0b111, 1, OP_REG);

    assert_eq!(
        reg_after(&[div], &[(2, 17), (3, 0)], 1),
        0xffff_ffff,
        "div by zero is -1"
    );
    assert_eq!(reg_after(&[divu], &[(2, 17), (3, 0)], 1), 0xffff_ffff);
    assert_eq!(
        reg_after(&[rem], &[(2, 17), (3, 0)], 1),
        17,
        "rem by zero is the dividend"
    );
    assert_eq!(reg_after(&[remu], &[(2, 17), (3, 0)], 1), 17);
}

#[test]
fn signed_division_overflow_is_defined() {
    let div = r_type(1, 3, 2, 0b100, 1, OP_REG);
    let rem = r_type(1, 3, 2, 0b110, 1, OP_REG);
    let min = 0x8000_0000u32;
    assert_eq!(reg_after(&[div], &[(2, min), (3, 0xffff_ffff)], 1), min);
    assert_eq!(reg_after(&[rem], &[(2, min), (3, 0xffff_ffff)], 1), 0);
}

#[test]
fn remainder_takes_the_sign_of_the_dividend() {
    let rem = r_type(1, 3, 2, 0b110, 1, OP_REG);
    // -7 % 2 == -1
    assert_eq!(
        reg_after(&[rem], &[(2, (-7i32) as u32), (3, 2)], 1),
        (-1i32) as u32
    );
    // 7 % -2 == 1
    assert_eq!(reg_after(&[rem], &[(2, 7), (3, (-2i32) as u32)], 1), 1);
}

#[test]
fn the_three_high_multiplies_differ() {
    let mulh = r_type(1, 3, 2, 0b001, 1, OP_REG);
    let mulhsu = r_type(1, 3, 2, 0b010, 1, OP_REG);
    let mulhu = r_type(1, 3, 2, 0b011, 1, OP_REG);
    let a = 0xffff_ffffu32; // -1 signed
    let b = 0x0000_0002u32;

    assert_eq!(
        reg_after(&[mulh], &[(2, a), (3, b)], 1),
        0xffff_ffff,
        "-1 * 2 high word"
    );
    assert_eq!(reg_after(&[mulhsu], &[(2, a), (3, b)], 1), 0xffff_ffff);
    assert_eq!(
        reg_after(&[mulhu], &[(2, a), (3, b)], 1),
        1,
        "0xffffffff * 2 unsigned"
    );
}

#[test]
fn mul_returns_the_low_word_and_wraps() {
    let mul = r_type(1, 3, 2, 0b000, 1, OP_REG);
    assert_eq!(
        reg_after(&[mul], &[(2, 0xffff_ffff), (3, 0xffff_ffff)], 1),
        1
    );
}

// ---------------------------------------------------------------- system

#[test]
fn ecall_and_ebreak_stop_the_hart() {
    let ecall = i_type(0, 0, 0b000, 0, OP_SYSTEM);
    let ebreak = i_type(1, 0, 0b000, 0, OP_SYSTEM);
    let mut hart = hart_with(&[ecall], &[]);
    assert!(matches!(hart.step(), Err(Trap::EnvironmentCall { .. })));
    let mut hart = hart_with(&[ebreak], &[]);
    assert!(matches!(hart.step(), Err(Trap::Breakpoint { .. })));
}

#[test]
fn an_unknown_encoding_is_an_illegal_instruction() {
    let mut hart = hart_with(&[0xffff_ffff], &[]);
    let err = hart.step().unwrap_err();
    assert!(
        matches!(err, Trap::IllegalInstruction { .. }),
        "got {err:?}"
    );
}

#[test]
fn fence_is_a_no_op_that_advances_pc() {
    let fence = i_type(0, 0, 0b000, 0, OP_FENCE);
    let hart = step_one(&[fence], &[]);
    assert_eq!(hart.pc, ENTRY + 4);
}

// ---------------------------------------------------------------- sanity

#[test]
fn a_straight_line_advances_pc_by_four_each_time() {
    let program = [
        i_type(1, 0, 0b000, 1, OP_IMM),
        i_type(2, 0, 0b000, 2, OP_IMM),
        i_type(3, 0, 0b000, 3, OP_IMM),
    ];
    let mut hart: Hart = hart_with(&program, &[]);
    for _ in 0..3 {
        hart.step().unwrap();
    }
    assert_eq!(hart.pc, ENTRY + 12);
    assert_eq!((hart.reg(1), hart.reg(2), hart.reg(3)), (1, 2, 3));
}
