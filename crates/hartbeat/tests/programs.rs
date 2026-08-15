//! Real programs, compiled by LLVM, run on the emulator, checked against the
//! same source running natively.
//!
//! This is the test that is not circular. Everywhere else in this repository the
//! instruction words were chosen and encoded by code in this repository, so a
//! consistent misreading of the specification would be invisible. Here the
//! instruction selection and the encoding are LLVM's, and the expected answers
//! come from compiling the identical source for the host and running it on real
//! hardware. Neither side of that comparison is this project's opinion about
//! RISC-V.
//!
//! The algorithms are literally the same file: `guests/src/algorithms.rs` is
//! `include!`d into the guest binaries and into this test.

mod common;

use hartbeat::{elf, Hart, Stop, Trap};

// Compiled for the host here, and for riscv32im inside the guest images.
include!("../../../guests/src/algorithms.rs");

const IN_BASE: u32 = 0x8001_0000;
const OUT_BASE: u32 = 0x8001_1000;
const PAGE: u32 = 4096;
const BUDGET: u64 = 5_000_000;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(format!("{name}.elf"))
}

/// Load a guest, seed its inputs, run it to `ebreak`, and return its output
/// page.
fn run_guest(name: &str, inputs: &[u32]) -> Vec<u32> {
    let path = fixture(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run tools/build-guests.sh)", path.display()));

    let mut hart = Hart::new(0);
    let entry = elf::load(&bytes, &mut hart.mem).expect("guest image should load");
    hart.pc = entry;

    // The pages the runtime contract promises: inputs, outputs, and a stack.
    let zeros = vec![0u8; PAGE as usize];
    hart.mem.write_slice(IN_BASE, &zeros);
    hart.mem.write_slice(OUT_BASE, &zeros);
    for page in 0..4u32 {
        hart.mem.write_slice(0x8001_C000 + page * PAGE, &zeros);
    }

    for (i, value) in inputs.iter().enumerate() {
        hart.mem
            .write(IN_BASE + (i as u32) * 4, *value, 4, 0)
            .expect("input page is mapped");
    }

    let stop = hart.run(BUDGET);
    match stop {
        Stop::Halted(Trap::Breakpoint { .. }) => {}
        other => panic!(
            "{name}: expected ebreak, got {other:?} after {} steps",
            hart.steps()
        ),
    }

    (0..64)
        .map(|i| hart.mem.read(OUT_BASE + i * 4, 4, 0).expect("output page"))
        .collect()
}

#[test]
fn arith_matches_the_host() {
    let cases: [(u32, u32); 8] = [
        (0, 0),
        (1, 1),
        (0xffff_ffff, 1),
        (0x8000_0000, 31),
        (0x1234_5678, 0x9abc_def0),
        (0xdead_beef, 4),
        (0x0000_00ff, 33),
        (0x7fff_ffff, 0xffff_ffff),
    ];
    for (a, b) in cases {
        let got = run_guest("arith", &[a, b]);
        let mut want = [0u32; 20];
        arith(a, b, &mut want);
        for (i, expected) in want.iter().enumerate() {
            assert_eq!(
                got[i], *expected,
                "arith({a:#x}, {b:#x}) output {i}: emulator {:#x}, host {:#x}",
                got[i], expected
            );
        }
    }
}

#[test]
fn sort_matches_the_host() {
    let mut seed = 0x1234_5678u32;
    for round in 0..8 {
        let mut values = [0u32; 32];
        for slot in values.iter_mut() {
            // A cheap deterministic spread, including negatives when read as i32.
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *slot = seed;
        }
        let got = run_guest("sort", &values);

        let mut want = values;
        let checksum = sort_and_checksum(&mut want);
        for (i, expected) in want.iter().enumerate() {
            assert_eq!(got[i], *expected, "round {round}, element {i}");
        }
        assert_eq!(got[32], checksum, "round {round} checksum");
    }
}

#[test]
fn control_flow_matches_the_host() {
    for seed in [
        0u32,
        1,
        2,
        7,
        27,
        97,
        0x7fff_ffff,
        0xffff_ffff,
        0x8000_0000,
        12345,
    ] {
        let got = run_guest("control", &[seed]);
        let mut want = [0u32; 6];
        control(seed, &mut want);
        for (i, expected) in want.iter().enumerate() {
            assert_eq!(got[i], *expected, "control({seed:#x}) output {i}");
        }
    }
}

/// Memory semantics, checked against hand-derived expectations rather than
/// against a host run, because the guest is about widths and sign extension at
/// specific addresses rather than about a computation.
#[test]
fn memops_matches_the_specified_widths() {
    for seed in [
        0x1234_5678u32,
        0xffff_ffff,
        0x0000_0080,
        0x8000_0000,
        0x00ff_00ff,
    ] {
        let got = run_guest("memops", &[seed]);
        let b0 = seed as u8;
        let h0 = seed as u16;

        assert_eq!(got[0], (b0 as i8) as i32 as u32, "lb sign-extends");
        assert_eq!(got[1], b0 as u32, "lbu zero-extends");
        assert_eq!(got[2], (h0 as i16) as i32 as u32, "lh sign-extends");
        assert_eq!(got[3], h0 as u32, "lhu zero-extends");
        assert_eq!(got[4], seed, "lw returns the whole word");
        assert_eq!(got[5], 0xaaaa_0000 | (h0 as u32), "sh leaves the high half");
        assert_eq!(
            got[6],
            0x5555_5555 & !0x00ff_0000 | (((seed >> 8) & 0xff) << 16),
            "sb touches one byte"
        );
        assert_eq!(got[7], ((seed >> 3) as u16) as u32, "aligned half at +2");
    }
}

/// The M extension through instructions the assembler emitted, including the two
/// cases Rust will not let a guest write.
///
/// The explicit zero checks are the specification written out, not a missed
/// `checked_div`; see the note on `muldiv` in the emulator.
#[allow(clippy::manual_checked_ops)]
#[test]
fn muldiv_matches_the_specification() {
    const MIN: u32 = 0x8000_0000;
    const NEG1: u32 = 0xffff_ffff;
    let cases: [(u32, u32); 9] = [
        (0, 0),
        (17, 5),
        (17, 0),
        (MIN, NEG1),
        (NEG1, NEG1),
        (NEG1, 2),
        (7, (-2i32) as u32),
        ((-7i32) as u32, 2),
        (0x1234_5678, 0x9abc_def0),
    ];
    for (a, b) in cases {
        let got = run_guest("muldiv", &[a, b]);

        assert_eq!(got[0], a.wrapping_mul(b), "mul({a:#x},{b:#x})");
        assert_eq!(
            got[1],
            (((a as i32 as i64) * (b as i32 as i64)) >> 32) as u32,
            "mulh({a:#x},{b:#x})"
        );
        assert_eq!(
            got[2],
            (((a as i32 as i64) * (b as i64)) >> 32) as u32,
            "mulhsu({a:#x},{b:#x})"
        );
        assert_eq!(got[3], (((a as u64) * (b as u64)) >> 32) as u32, "mulhu");

        let div = if b == 0 {
            NEG1
        } else if a == MIN && b == NEG1 {
            MIN
        } else {
            ((a as i32).wrapping_div(b as i32)) as u32
        };
        let rem = if b == 0 {
            a
        } else if a == MIN && b == NEG1 {
            0
        } else {
            ((a as i32).wrapping_rem(b as i32)) as u32
        };
        assert_eq!(got[4], div, "div({a:#x},{b:#x})");
        assert_eq!(got[5], if b == 0 { NEG1 } else { a / b }, "divu");
        assert_eq!(got[6], rem, "rem({a:#x},{b:#x})");
        assert_eq!(got[7], if b == 0 { a } else { a % b }, "remu");

        // Reached whatever the inputs were.
        assert_eq!(got[8], NEG1, "div by zero is all ones");
        assert_eq!(got[9], NEG1, "divu by zero is all ones");
        assert_eq!(got[10], a, "rem by zero is the dividend");
        assert_eq!(got[11], a, "remu by zero is the dividend");
        assert_eq!(got[12], MIN, "div overflow stays at the minimum");
        assert_eq!(got[13], 0, "rem overflow is zero");
    }
}

/// The images must actually be doing work. A guest that halted in ten
/// instructions would pass every assertion above if the compiler had folded it,
/// so the instruction count is checked too.
#[test]
fn the_guests_execute_a_real_number_of_instructions() {
    let mut hart = Hart::new(0);
    let bytes = std::fs::read(fixture("sort")).expect("sort image");
    let entry = elf::load(&bytes, &mut hart.mem).unwrap();
    hart.pc = entry;
    let zeros = vec![0u8; PAGE as usize];
    hart.mem.write_slice(IN_BASE, &zeros);
    hart.mem.write_slice(OUT_BASE, &zeros);
    for page in 0..4u32 {
        hart.mem.write_slice(0x8001_C000 + page * PAGE, &zeros);
    }
    // Reverse order, the worst case for insertion sort.
    for i in 0..32u32 {
        hart.mem.write(IN_BASE + i * 4, 32 - i, 4, 0).unwrap();
    }
    hart.run(BUDGET);
    assert!(
        hart.steps() > 2000,
        "sort ran only {} instructions, which means it was folded away",
        hart.steps()
    );
    println!("sort executed {} instructions", hart.steps());
}
