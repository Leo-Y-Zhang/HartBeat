//! Lockstep differential execution.
//!
//! Both implementations run the same program one instruction at a time, and the
//! whole architectural state is compared after every step. A divergence is
//! reported at the instruction that caused it rather than at the end of the run,
//! which is the difference between a bug report and a puzzle.
//!
//! The programs are generated, not written. A generated program does things a
//! person writing tests does not think to do -- shifting by a register that
//! holds 37, jumping to an address that is also a data pointer, dividing by a
//! value that happens to be zero -- and it does them thousands of times.

mod common;

use common::*;
use hartbeat::{Hart, Trap};
use hartbeat_ref::{RefHart, RefTrap};

const CODE_WORDS: usize = 1024;
const PAGE: usize = 4096;

/// The two crates report traps in their own vocabularies. This is the only
/// place they are related, and it is deliberately coarse: what matters is that
/// both stopped for the same *reason*, not that they formatted it alike.
fn same_trap(primary: Trap, reference: RefTrap) -> bool {
    matches!(
        (primary, reference),
        (Trap::IllegalInstruction { .. }, RefTrap::Illegal)
            | (Trap::MisalignedAccess { .. }, RefTrap::Misaligned)
            | (Trap::AccessFault { .. }, RefTrap::Fault)
            | (Trap::EnvironmentCall { .. }, RefTrap::Ecall)
            | (Trap::Breakpoint { .. }, RefTrap::Ebreak)
    )
}

/// A tiny xorshift, so that a failing case is reproducible from its seed and the
/// crate keeps its zero dependencies.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

/// Registers held back from being written, so that they keep pointing where they
/// were initialised.
///
/// Without these the campaign is worthless and looks fine. A first version let
/// every instruction write every register and took its base addresses from
/// whatever happened to be there: the average program trapped after **5.7
/// instructions**, so 400 programs exercised almost nothing while every test
/// passed. Reserving three pointers took the same campaign to hundreds of
/// instructions per program. The lesson is the assertion at the bottom of this
/// file, which now fails if the average run is short.
const CODE_PTR: u32 = 29;
const DATA_PTR_A: u32 = 30;
const DATA_PTR_B: u32 = 31;

/// One random but mostly-legal instruction.
///
/// Mostly, not entirely: one word in sixty-four is fully random, so the two
/// implementations also have to agree about what is *not* an instruction. The
/// dedicated 200,000-word legality test carries most of that weight; here a
/// higher rate would just end runs early.
fn random_instruction(rng: &mut Rng) -> u32 {
    if rng.below(64) == 0 {
        return rng.next_u32();
    }
    // Never a destination, so the pointer registers survive the whole run.
    let rd = rng.below(CODE_PTR);
    let rs1 = rng.below(32);
    let rs2 = rng.below(32);
    match rng.below(9) {
        0 => u_type(rng.next_u32(), rd, OP_LUI),
        1 => u_type(rng.next_u32(), rd, OP_AUIPC),
        // Jumps and branches stay inside the code page so that runs are long
        // enough to be interesting. Leaving it is legal and both agree on the
        // fault, but the run ends there.
        2 => j_type(((rng.below(128) as i32) - 64) * 4, rd, OP_JAL),
        // Through the code pointer, so the target is an instruction rather than
        // wherever an arithmetic result happened to land.
        3 => i_type(
            ((rng.below(128) as i32) - 64) * 4,
            CODE_PTR,
            0b000,
            rd,
            OP_JALR,
        ),
        4 => {
            let f3 = [0b000u32, 0b001, 0b100, 0b101, 0b110, 0b111][rng.below(6) as usize];
            b_type(((rng.below(64) as i32) - 32) * 4, rs2, rs1, f3, OP_BRANCH)
        }
        5 => {
            let f3 = [0b000u32, 0b001, 0b010, 0b100, 0b101][rng.below(5) as usize];
            let (base, offset) = memory_operand(rng, rs1);
            i_type(offset, base, f3, rd, OP_LOAD)
        }
        6 => {
            let f3 = [0b000u32, 0b001, 0b010][rng.below(3) as usize];
            let (base, offset) = memory_operand(rng, rs1);
            s_type(offset, rs2, base, f3, OP_STORE)
        }
        7 => match rng.below(8) {
            0 => i_type((rng.next_u32() as i32) >> 20, rs1, 0b000, rd, OP_IMM),
            1 => i_type((rng.next_u32() as i32) >> 20, rs1, 0b010, rd, OP_IMM),
            2 => i_type((rng.next_u32() as i32) >> 20, rs1, 0b011, rd, OP_IMM),
            3 => i_type((rng.next_u32() as i32) >> 20, rs1, 0b100, rd, OP_IMM),
            4 => i_type((rng.next_u32() as i32) >> 20, rs1, 0b110, rd, OP_IMM),
            5 => i_type((rng.next_u32() as i32) >> 20, rs1, 0b111, rd, OP_IMM),
            6 => i_type(rng.below(32) as i32, rs1, 0b001, rd, OP_IMM),
            _ => {
                let arith = rng.below(2) == 1;
                let imm = ((if arith { 0b0100000u32 } else { 0 }) << 5) | rng.below(32);
                i_type(imm as i32, rs1, 0b101, rd, OP_IMM)
            }
        },
        _ => match rng.below(3) {
            0 => {
                let f3 = rng.below(8);
                r_type(0b0000000, rs2, rs1, f3, rd, OP_REG)
            }
            1 => {
                let f3 = if rng.below(2) == 0 { 0b000 } else { 0b101 };
                r_type(0b0100000, rs2, rs1, f3, rd, OP_REG)
            }
            _ => r_type(0b0000001, rs2, rs1, rng.below(8), rd, OP_REG),
        },
    }
}

/// Where a load or store points.
///
/// Usually one of the two data pointers with a word-aligned offset, so the
/// access lands in the data page and the run continues. One in twelve uses an
/// arbitrary register and an arbitrary offset, which mostly faults or
/// misaligns -- both implementations still have to agree about that, and this
/// keeps the trap paths in the campaign without ending every run in three
/// instructions.
fn memory_operand(rng: &mut Rng, wild: u32) -> (u32, i32) {
    if rng.below(12) == 0 {
        return (wild, (rng.below(64) as i32) - 32);
    }
    let base = if rng.below(2) == 0 {
        DATA_PTR_A
    } else {
        DATA_PTR_B
    };
    (base, ((rng.below(64) as i32) - 32) * 4)
}

/// Initial register values. The three pointer registers are aimed at the middle
/// of the pages they address, so an offset in either direction stays inside.
fn random_regs(rng: &mut Rng) -> [u32; 32] {
    let mut regs = [0u32; 32];
    for (i, slot) in regs.iter_mut().enumerate() {
        if i == 0 {
            continue;
        }
        *slot = match rng.below(4) {
            0 => DATA.wrapping_add(rng.below(PAGE as u32 - 64) & !3),
            1 => rng.below(64),
            2 => rng.next_u32(),
            _ => (rng.next_u32() as i32 >> 24) as u32,
        };
    }
    regs[CODE_PTR as usize] = ENTRY + (CODE_WORDS as u32 / 2) * 4;
    regs[DATA_PTR_A as usize] = DATA + PAGE as u32 / 4;
    regs[DATA_PTR_B as usize] = DATA + PAGE as u32 / 2;
    regs
}

struct Pair {
    primary: Hart,
    reference: RefHart,
}

/// Build both harts over identical memory.
///
/// Both pages are written in full and both are page-aligned, so the two
/// implementations end up with exactly the same set of mapped bytes despite one
/// allocating pages and the other individual bytes. Without that, a load just
/// past the program would fault in one and not the other, and the harness would
/// report a divergence that is really a difference in allocation granularity.
fn build(program: &[u32], regs: [u32; 32], data: &[u8]) -> Pair {
    let mut code = vec![0u8; PAGE];
    for (i, word) in program.iter().enumerate() {
        code[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }

    let mut primary = Hart::new(ENTRY);
    primary.mem.write_slice(ENTRY, &code);
    primary.mem.write_slice(DATA, data);

    let mut reference = RefHart::new(ENTRY);
    reference.write_bytes(ENTRY, &code);
    reference.write_bytes(DATA, data);

    for i in 1..32u8 {
        primary.set_reg(i, regs[i as usize]);
        reference.set(i, regs[i as usize]);
    }

    Pair { primary, reference }
}

/// Run both, comparing after every instruction. `Err` describes the first
/// divergence.
fn lockstep(pair: &mut Pair, budget: usize) -> Result<usize, String> {
    for step in 0..budget {
        let pc = pair.primary.pc;
        let word = pair.primary.mem.read(pc, 4, pc).unwrap_or(0);

        let a = pair.primary.step();
        let b = pair.reference.step();

        match (a, b) {
            (Err(pa), Err(rb)) => {
                return if same_trap(pa, rb) {
                    Ok(step)
                } else {
                    Err(format!(
                        "step {step}: pc {pc:#010x} word {word:#010x}: \
                         primary trapped {pa:?} but reference trapped {rb:?}"
                    ))
                };
            }
            (Err(pa), Ok(())) => {
                return Err(format!(
                    "step {step}: pc {pc:#010x} word {word:#010x}: \
                     primary trapped {pa:?} but reference continued"
                ))
            }
            (Ok(()), Err(rb)) => {
                return Err(format!(
                    "step {step}: pc {pc:#010x} word {word:#010x}: \
                     reference trapped {rb:?} but primary continued"
                ))
            }
            (Ok(()), Ok(())) => {}
        }

        if pair.primary.pc != pair.reference.pc {
            return Err(format!(
                "step {step}: pc {pc:#010x} word {word:#010x}: pc diverged, \
                 primary {:#010x} reference {:#010x}",
                pair.primary.pc, pair.reference.pc
            ));
        }
        let pr = pair.primary.regs();
        let rr = pair.reference.regs();
        for i in 0..32 {
            if pr[i] != rr[i] {
                return Err(format!(
                    "step {step}: pc {pc:#010x} word {word:#010x}: x{i} diverged, \
                     primary {:#010x} reference {:#010x}",
                    pr[i], rr[i]
                ));
            }
        }
    }
    Ok(budget)
}

/// Compare every byte of the data page after the run, so a store that is never
/// read back is still checked.
fn compare_memory(pair: &Pair) -> Result<(), String> {
    for offset in 0..PAGE as u32 {
        let addr = DATA + offset;
        let a = pair.primary.mem.read(addr, 1, 0).ok();
        let b = pair.reference.peek(addr).map(u32::from);
        if a != b {
            return Err(format!("data byte {addr:#010x} diverged: {a:?} vs {b:?}"));
        }
    }
    Ok(())
}

fn run_campaign(seed_base: u64, programs: usize, budget: usize) -> (usize, usize) {
    let mut executed = 0usize;
    for case in 0..programs {
        let seed = seed_base
            .wrapping_add(case as u64)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut rng = Rng::new(seed);
        let program: Vec<u32> = (0..CODE_WORDS)
            .map(|_| random_instruction(&mut rng))
            .collect();
        let regs = random_regs(&mut rng);
        let mut data = vec![0u8; PAGE];
        for byte in data.iter_mut() {
            *byte = rng.next_u32() as u8;
        }

        let mut pair = build(&program, regs, &data);
        match lockstep(&mut pair, budget) {
            Ok(steps) => executed += steps,
            Err(why) => panic!("case {case} (seed {seed:#x}) diverged: {why}"),
        }
        if let Err(why) = compare_memory(&pair) {
            panic!("case {case} (seed {seed:#x}) diverged after the run: {why}");
        }
    }
    (programs, executed)
}

#[test]
fn generated_programs_agree() {
    let (programs, executed) = run_campaign(0x5eed_0001, 400, 2000);
    // A campaign of programs that trap immediately agrees about everything and
    // proves nothing. This bound is the guard against that, and it is set high
    // enough that the original 5.7-instructions-per-program generator would fail
    // it rather than squeak past.
    assert!(
        executed > programs * 100,
        "campaign executed {executed} instructions over {programs} programs, \
         an average of {:.1} each -- the generator is producing programs that stop \
         almost immediately, so this test is not testing anything",
        executed as f64 / programs as f64
    );
    println!(
        "{programs} programs, {executed} instructions executed in lockstep \
         ({:.0} per program)",
        executed as f64 / programs as f64
    );
}

#[test]
fn a_different_seed_agrees_too() {
    let (programs, executed) = run_campaign(0xa5a5_1234, 200, 2000);
    assert!(executed > programs * 100);
}

/// Nothing but noise: neither implementation may accept an encoding the other
/// rejects.
#[test]
fn fully_random_words_agree_on_legality() {
    let mut rng = Rng::new(0xdead_beef);
    let mut illegal = 0usize;
    let mut legal = 0usize;
    for _ in 0..200_000 {
        let word = rng.next_u32();
        let primary = hartbeat::decode(word).is_some();
        // The reference has no decoder to ask, so it is asked the only way it
        // can be: put the word in front of it and see whether it refuses.
        let mut hart = RefHart::new(ENTRY);
        hart.write_bytes(ENTRY, &word.to_le_bytes());
        let reference = !matches!(hart.step(), Err(RefTrap::Illegal));
        assert_eq!(
            primary, reference,
            "word {word:#010x}: primary says legal={primary}, reference says legal={reference}"
        );
        if primary {
            legal += 1
        } else {
            illegal += 1
        }
    }
    println!("{legal} legal, {illegal} illegal encodings agreed on");
    assert!(legal > 1000, "the sample found almost no legal encodings");
}
