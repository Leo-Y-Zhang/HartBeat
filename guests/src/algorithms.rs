// The computations the guests perform, as plain integer functions.
//
// This file is compiled twice from one source: once for
// riscv32im-unknown-none-elf, where it becomes the guest image the emulator
// executes, and once for the host, where the test calls it directly. The
// emulator is correct on this workload only if the two agree bit for bit.
//
// That is the strongest oracle available here, and the reason is that nobody
// involved has an opinion about RISC-V. LLVM chose which RISC-V instructions
// implement `rotate_left` and `count_ones`; the host answer comes from the same
// source compiled for a completely different machine and run on real silicon.
// A misunderstanding of the instruction set on this project's part cannot be
// present on both sides, because this project wrote neither side's semantics.
//
// No `std`, no allocation, no floating point: it has to compile for a bare
// RV32IM target.

pub fn arith(a: u32, b: u32, out: &mut [u32; 20]) {
    out[0] = a.wrapping_add(b);
    out[1] = a.wrapping_sub(b);
    out[2] = a & b;
    out[3] = a | b;
    out[4] = a ^ b;
    out[5] = a << (b & 31);
    out[6] = a >> (b & 31);
    out[7] = ((a as i32) >> (b & 31)) as u32;
    out[8] = ((a as i32) < (b as i32)) as u32;
    out[9] = (a < b) as u32;
    out[10] = a.wrapping_add(0x7ff);
    out[11] = a.wrapping_sub(0x800);
    out[12] = (a as i32).wrapping_neg() as u32;
    out[13] = !a;
    // RV32IM has none of these, so LLVM expands each into base-ISA sequences.
    out[14] = a.rotate_left(b & 31);
    out[15] = a.count_ones();
    out[16] = a.leading_zeros();
    out[17] = a.trailing_zeros();
    out[18] = a.swap_bytes();
    out[19] = a.reverse_bits();
}

/// Insertion sort, ascending by signed comparison, plus a checksum that depends
/// on every element and on multiplication.
pub fn sort_and_checksum(values: &mut [u32; 32]) -> u32 {
    for i in 1..values.len() {
        let key = values[i];
        let mut j = i;
        while j > 0 && (values[j - 1] as i32) > (key as i32) {
            values[j] = values[j - 1];
            j -= 1;
        }
        values[j] = key;
    }

    let mut sum: u32 = 0;
    let mut i = 0;
    while i < values.len() {
        sum = sum
            .wrapping_mul(31)
            .wrapping_add(values[i].wrapping_mul(i as u32 + 1));
        i += 1;
    }
    sum
}

/// Data-dependent control flow: a Collatz stopping time, a nested loop whose
/// trip count comes from the input, and a walk over the input's bits.
pub fn control(seed: u32, out: &mut [u32; 6]) {
    let mut n: u32 = if seed == 0 { 1 } else { seed };
    let mut steps: u32 = 0;
    while n != 1 && steps < 5000 {
        n = if n % 2 == 0 {
            n / 2
        } else {
            n.wrapping_mul(3).wrapping_add(1)
        };
        steps += 1;
    }
    out[0] = steps;
    out[1] = n;

    let outer = (seed & 0x1f) + 1;
    let mut acc: u32 = 0;
    let mut i = 0;
    while i < outer {
        let mut j = 0;
        while j < outer {
            acc = acc.wrapping_add(i.wrapping_mul(j).wrapping_add(1));
            j += 1;
        }
        i += 1;
    }
    out[2] = acc;
    out[3] = outer;

    let mut ones: u32 = 0;
    let mut runs: u32 = 0;
    let mut previous = false;
    let mut bit = 0;
    while bit < 32 {
        let set = (seed >> bit) & 1 == 1;
        if set {
            ones += 1;
            if !previous {
                runs += 1;
            }
        }
        previous = set;
        bit += 1;
    }
    out[4] = ones;
    out[5] = runs;
}
