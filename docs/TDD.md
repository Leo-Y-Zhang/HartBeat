# TDD — HartBeat

## Shape

```
crates/hartbeat/
  decode.rs   word  -> typed Instr        (strict; unknown encodings are rejected)
  exec.rs     Instr -> state change       (including advancing pc)
  hart.rs     registers, pc, step, run
  mem.rs      sparse pages, traps on unmapped and misaligned access
  elf.rs      ELF32 PT_LOAD segments      (unverified boundary)
  bin/        the command line

crates/hartbeat-ref/
  lib.rs      the whole thing, deliberately unlike the above

crates/hartbeat/tests/
  spec_vectors.rs   hand-written cases from the specification
  lockstep.rs       generated programs, both interpreters, compared per instruction
  programs.rs       LLVM-compiled guests, compared against the host
```

## Decisions, and why

**`pc` is advanced by the executor, not by the caller.** A design where the
caller adds four afterwards has to special-case jumps and branches, which are
exactly the instructions most likely to be wrong. Here every instruction sets
`pc` and there is no default to forget to override.

**`x0` is enforced in the reader.** `Hart::reg` returns zero for index zero
regardless of what is stored, so no instruction can forget it. There are guards
in the writer and the state dump too; the mutation campaign shows that removing
any one of the three changes nothing observable, which is worth knowing and is
recorded rather than tidied away.

**Unmapped memory faults; it does not read as zero.** A guest reading
uninitialised memory has a bug, and returning zero would hide it behind
plausible-looking output.

**Misaligned accesses trap.** The base ISA permits a hart either to handle these
or to trap. Trapping is the stricter choice. Both interpreters had to make the
*same* choice for the comparison to mean anything, so this is a place where they
are not independent, and `trap.rs` says so at the definition.

**Anything outside RV32IM is rejected.** Including `fence.i` and the CSR
instructions. Accepting them as no-ops would let a program that depends on them
appear to work.

## Why there are two interpreters

Two implementations catch nothing unless their mistakes are uncorrelated, so the
differences in `hartbeat-ref` are deliberate rather than stylistic:

| | `hartbeat` | `hartbeat-ref` |
| --- | --- | --- |
| Decoding | word to a typed `Instr`, then execute | fields extracted at the point of use |
| Dispatch | match on the enum | flat match on `(opcode, funct3, funct7)` |
| Immediates | mask the bits out, then sign-extend with a shift pair | arithmetic shift of the whole word does the extension |
| Memory | page table | byte map |

The two are still not independent of *me*. That limitation is the reason the
third gate exists, and `MUTATIONS.md` demonstrates it concretely rather than
claiming it.

## The generated-program campaign, and the bug in it

The lockstep campaign generates programs rather than writing them, and the first
version of the generator was worthless in a way that looked fine.

Every instruction could write every register, and loads and stores took their
base addresses from whatever happened to be in `rs1`. Almost every memory access
faulted immediately, so **the average program trapped after 5.7 instructions**:
400 programs executed 2,262 instructions between them, agreed about everything,
and the test passed.

Three registers are now held back from being written and initialised to point at
the code page and the data page, so accesses land somewhere and runs continue.
The same campaign now executes **252,428 instructions, 631 per program** — 112
times the coverage, still with no divergence. The assertion at the bottom of
`lockstep.rs` now fails if the average run is short, so the failure mode cannot
come back silently.

The general lesson is that a passing differential test says nothing until you
know how much it executed, and the number is worth asserting on.

## Gates

| Gate | What it catches | Where |
| --- | --- | --- |
| Specification cases | Semantics an implementation plausibly gets wrong | `spec_vectors.rs` |
| Lockstep | Any per-instruction disagreement between two implementations, over generated programs | `lockstep.rs` |
| Legality agreement | A decoder too permissive or too strict, over 200,000 random words | `lockstep.rs` |
| Compiled programs | A misreading of the specification shared by everything in this repository | `programs.rs` |
| Mutation campaign | A gate that is not actually load-bearing | `tools/mutations.py` |
| MSRV build | A stated minimum Rust version nobody ever built with | CI |
| gitleaks, pinned binary, full history | Secrets | CI |

The mutation campaign runs in CI and the regenerated `MUTATIONS.md` must match
the committed one, so the evidence is re-derived on hardware that is not the
author's rather than taken on trust.

The secret scan uses the pinned `gitleaks` binary over `--log-opts=--all` rather
than the published action, which scans only the pushed range: a secret committed
once and never touched again is invisible to the action, and on a repository's
first push it fails outright because the root commit has no parent.

## The unverified edges

`elf.rs` decides what bytes become the program, and `Parse`-like code of that
kind is where a checker's guarantees stop. It refuses anything it does not fully
understand rather than guessing, which is a design choice and not a proof.

The guest runtime contract — entry, input and output addresses, `ebreak` to
finish — is a convention between the linker script and the test harness. A guest
that panics loops rather than reaching `ebreak`, so a panicking guest shows up as
a step-budget expiry and cannot be mistaken for a passing one.
