# PRD — HartBeat

## The problem

Writing a RISC-V interpreter that runs a compiled program is a weekend's work and
proves very little. The interesting question is the one after it: how would you
know if it were wrong?

An emulator is unusually good at hiding its own bugs. It runs almost everything
correctly, because almost everything uses the ten instructions that are hard to
get wrong, and then it produces a wrong answer on the one program that shifts by
a register holding 33, or divides by zero, or takes an indirect call through an
odd address. Nothing crashes. The output is simply wrong, and nothing in the
system is in a position to notice.

So the deliverable here is not the interpreter. It is a defensible answer to
"why do you believe this is correct", and an interpreter is what that answer is
about.

## Users

- Someone who wants to see what a verification story looks like when it is built
  deliberately rather than accumulated.
- Me, wanting an artefact below the level of the code I usually write, with the
  same standard of evidence.

## Success criteria

- RV32I and the M extension: 48 instructions, decoded strictly, with anything
  outside them rejected rather than approximated.
- Three gates that fail for different reasons, each demonstrably catching
  something the others miss.
- Real compiled programs, produced by a toolchain with no connection to this
  project, executed and checked against an oracle that is also not this project's
  opinion.
- Every gate observed failing, with the evidence regenerated in CI rather than
  asserted in prose.
- No dependencies, and the stated minimum supported Rust version actually built.

All five hold. The mutation table in `MUTATIONS.md` is the evidence for the
second and fourth.

## Explicit non-goals

- **A fast emulator.** A match on a decoded enum and a `BTreeMap` page table.
  Threaded dispatch or a JIT would make the interpreter harder to read and would
  not make the verification argument any better.
- **Privileged architecture.** No CSRs, no interrupts, no address translation.
  A trap ends the run and reports why.
- **Compressed or floating-point extensions.** RV32IM only.
- **Passing the official conformance suite.** Wanted, and honestly out of reach
  here: `riscv-tests` needs a GNU cross-toolchain that will not install on this
  machine. `MUTATIONS.md` names the one surviving mutation that a conformance
  suite would have caught, so the cost of the gap is on the record rather than
  glossed.
- **Verification in the formal sense.** These are tests. The proof-shaped version
  of this kind of argument is a different repository.

## The decision that shaped it

The obvious design is one interpreter and a lot of tests. What is here instead is
two interpreters and three gates, because of a specific worry: tests written by
the person who wrote the emulator, using encoders written by the same person,
check that the code matches its author's understanding and nothing more. The
compiled-program gate exists to break that circle, and the mutation campaign
exists to prove the circle was really broken rather than redrawn.
