# HartBeat

An RV32IM interpreter in Rust, and — the part that is actually the point — three
independent ways of checking that it is right.

A hart is what RISC-V calls a hardware thread. Writing one that runs a compiled
program is a weekend; being able to say why you believe it is correct is the
work, because an emulator that is subtly wrong runs almost everything perfectly
and then quietly produces a wrong answer.

48 instructions: the RV32I base integer set and the M extension. No dependencies
outside the standard library.

## How it is checked

**1. Hand-written cases from the specification.** 28 of them, chosen because they
are where a plausible implementation differs from the specified one: arithmetic
versus logical shift, signed versus unsigned comparison, shift amounts masked to
five bits, `jalr` clearing the low bit of its target, division by zero returning
all ones rather than trapping, `i32::MIN / -1` staying at the minimum. Code that
runs correctly on ordinary programs can be wrong about every one of these,
because ordinary programs rarely divide by zero.

**2. Lockstep against a second interpreter.** `hartbeat-ref` is a second RV32IM
implementation written to be *unlike* the first: no decode step at all, fields
pulled out of the word at the point of use, immediates derived by arithmetic
shift rather than mask-and-extend, and its own byte-map memory instead of a page
table. The two run generated programs one instruction at a time and the whole
architectural state — 32 registers and the pc — is compared after every step,
with the data page compared afterwards as well. A divergence is reported at the
instruction that caused it:

```
step 2: pc 0x80000008 word 0x41535133: x2 diverged, primary 0xffffffff reference 0x001fffff
```

**252,428 instructions across 400 generated programs**, a second campaign on a
different seed, and **200,000 random words** on which both must agree about what
is not an instruction.

**3. Real programs compiled by LLVM.** Five freestanding Rust programs are
compiled for `riscv32im-unknown-none-elf`, loaded from ELF, and run. The expected
answers come from compiling *the same source file* for the host and running it on
real hardware. `guests/src/algorithms.rs` is `include!`d into both the guest
images and the test.

This third gate exists because the first two share an author. The decoder, the
reference interpreter and the test encoder were all written by the same person
from the same document, so a misreading of the specification appears in all three
and they agree with each other perfectly. Here nobody involved has an opinion
about RISC-V: LLVM chose the instructions and encoded them, and the host answer
comes from a different machine entirely.

## Does any of it work?

`MUTATIONS.md` records twelve deliberate breakages, the gates run separately, and
which of them noticed. **Eleven of twelve killed**, and the table is the argument
for keeping all three gates, because each one catches something no other does:

| Mutation | spec vectors | lockstep | programs |
| --- | :---: | :---: | :---: |
| `jalr` stops clearing the low bit of its target | **caught** | - | - |
| Half-word stores write four bytes | - | **caught** | **caught** |
| Both interpreters *and* the test encoder misread the S-type immediate | - | - | **caught** |

The third row is the one worth staring at. Making the decoder, the second
interpreter and the test encoder misread a store's offset in the same way — a
misreading, not a typo — passes 28 specification cases and a quarter of a million
lockstep instructions without a murmur. The compiled programs catch it
immediately, because LLVM did not make the same mistake.

The survivor is recorded too: widening `srai`'s shift amount to six bits, an RV64
habit, makes both interpreters accept encodings the specification reserves. It
changes nothing about any valid program, so no amount of running valid programs
can see it, and the two interpreters agree because both were changed. Catching it
needs a conformance suite that asserts reserved encodings are refused, which this
repository does not have.

## Running it

```
cargo test --release              # 38 tests
cargo run --release -p hartbeat -- fixtures/sort.elf --dump-regs
```

The generated programs and the mutation campaign are reproducible from their
seeds and re-run in CI on hardware that is not mine.

## What it does not do

- **Privileged anything.** No CSRs, no traps to a handler, no interrupts, no
  virtual memory. A trap stops the hart and tells the caller why.
- **Compressed or floating point.** No C extension, no F or D. Encodings outside
  RV32IM are rejected rather than approximated, including `fence.i` and the CSR
  instructions, because accepting them as no-ops would let a program that depends
  on them appear to work.
- **A conformance suite.** The official RISC-V tests need a GNU cross-toolchain
  this machine does not have. The survivor above is exactly what that would
  catch, and it is the first thing I would add.
- **Speed.** A plain match interpreter with a `BTreeMap` page table. It is written
  to be read and compared against, not to be fast.
- **Verified anything.** These are tests, not proofs. For the other approach —
  a machine-checked proof rather than a test campaign — see
  [Ratified](https://github.com/Leo-Y-Zhang/Ratified).

## Layout

| Path | What is in it |
| --- | --- |
| `crates/hartbeat/src/` | The interpreter: decode, execute, memory, traps, ELF loader, CLI |
| `crates/hartbeat-ref/` | The second interpreter, written differently on purpose |
| `crates/hartbeat/tests/spec_vectors.rs` | The hand-written specification cases |
| `crates/hartbeat/tests/lockstep.rs` | Generated programs, compared instruction by instruction |
| `crates/hartbeat/tests/programs.rs` | The compiled guests, checked against the host |
| `crates/hartbeat/tests/elf_loader.rs` | What the loader refuses |
| `guests/` | The freestanding Rust programs and their shared algorithms |
| `fixtures/` | The compiled guest images, with provenance |
| `tools/mutations.py` | Breaks the emulator twelve ways and records what noticed |

1,327 lines of implementation, 1,131 lines of tests.

## Licence

MIT. See `LICENSE`.
