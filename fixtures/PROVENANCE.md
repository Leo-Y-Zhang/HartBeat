# The guest images

Five freestanding RV32IM programs, compiled from `guests/` by `rustc` through
LLVM for `riscv32im-unknown-none-elf`. Rebuild them with:

```
rustup target add riscv32im-unknown-none-elf
tools/build-guests.sh
```

They are committed rather than built during the test run for two reasons. A
different `rustc` selects different instructions, so building them in CI would
mean the workload changed silently whenever the toolchain moved; and a reader
should be able to run the tests without a cross-compilation target installed.
CI does both — it runs the committed images, and separately rebuilds them from
source and runs the tests again against the fresh ones.

| Image | What it exercises |
| --- | --- |
| `arith.elf` | The ALU, plus `rotate_left`, `count_ones`, `leading_zeros` and `reverse_bits`, none of which RV32IM has — LLVM expands each into base-ISA sequences nobody here chose. |
| `muldiv.elf` | The M extension through inline assembly, including division by zero and `i32::MIN / -1`, which safe Rust cannot express. |
| `memops.elf` | Loads and stores at every width, sign and zero extension, and partial-word stores leaving their neighbours alone. |
| `sort.elf` | Insertion sort over 32 words: nested loops, a data-dependent branch, and computed addresses interacting. |
| `control.elf` | A Collatz stopping time, a nested loop whose trip count comes from the input, and a walk over the input's bits. |

## Why these are the tests that count

Everywhere else in this repository the instruction words were chosen and encoded
by code in this repository, so a consistent misreading of the specification would
be invisible: the decoder, the second interpreter and the test encoder would all
agree with each other and all be wrong. Here the instruction selection and the
encoding are LLVM's, and the expected answers come from compiling the *same
source file* — `guests/src/algorithms.rs` — for the host and running it on real
hardware.

`MUTATIONS.md` shows that this is not a theoretical concern. Making the decoder,
the reference interpreter and the test encoder misread the S-type immediate in
the same way passes every other gate in the repository and is caught here alone.

## The contract with the emulator

- Entry at `0x80000000`, which the linker script fixes.
- Inputs at `0x80010000`, one word each, written by the harness before the run.
- Outputs at `0x80011000`, one word each.
- `ebreak` when finished. A guest that panics loops instead, so a panicking guest
  cannot be mistaken for a finished one — the harness sees the step budget expire
  rather than a halt.

Inputs are read with `read_volatile`. Without that, LLVM folds each program into
a handful of stores of constants: an early version of `arith` compiled to four
instructions, one of which was the answer.
