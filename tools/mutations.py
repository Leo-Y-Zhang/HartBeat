"""Break the emulator on purpose, and record which gate notices.

A test that has never been observed failing is decoration. This applies each
mutation to a green tree, runs the three gates separately, records which of them
caught it, and reverts.

The interesting column is which gate fired. Three of the mutations are applied to
*both* interpreters at once, which is what a misreading of the specification
would look like -- the lockstep comparison is blind to those by construction, and
only the compiled-program gate can see them.

    python tools/mutations.py
"""

import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

ENV = dict(os.environ)
_cargo = os.path.join(os.path.expanduser("~"), ".cargo", "bin")
if os.path.isdir(_cargo):
    ENV["PATH"] = _cargo + os.pathsep + ENV.get("PATH", "")

EXEC = "crates/hartbeat/src/exec.rs"
HART = "crates/hartbeat/src/hart.rs"
DECODE = "crates/hartbeat/src/decode.rs"
REF = "crates/hartbeat-ref/src/lib.rs"
COMMON = "crates/hartbeat/tests/common/mod.rs"

GATES = ["spec_vectors", "lockstep", "programs"]

MUTATIONS = [
    dict(
        name="Arithmetic right shift becomes logical",
        edits=[(EXEC, "AluOp::Sra => ((a as i32) >> (b & 0x1f)) as u32,",
                      "AluOp::Sra => a >> (b & 0x1f),")],
        why="The difference only shows on negative values, which is most of the "
            "interesting ones.",
    ),
    dict(
        name="A shift of 32 or more produces zero instead of wrapping",
        edits=[(EXEC, "AluOp::Sll => a << (b & 0x1f),",
                      "AluOp::Sll => a.checked_shl(b).unwrap_or(0),")],
        why="Shifting by 33 must shift by one. Note that the obvious mutation -- "
            "replacing the mask with `wrapping_shl` -- is not a mutation at all, "
            "because Rust's `wrapping_shl` masks to the width itself. That one "
            "was tried first and survived, for that reason and not because "
            "anything was untested.",
    ),
    dict(
        name="JALR stops clearing the low bit of its target",
        edits=[(EXEC, "let target = hart.reg(rs1).wrapping_add(imm as u32) & !1;",
                      "let target = hart.reg(rs1).wrapping_add(imm as u32);")],
        why="Every indirect call goes through this, and the bit is only ever set "
            "when the offset is odd -- which a compiler does emit.",
    ),
    dict(
        name="Unsigned comparison becomes signed",
        edits=[(EXEC, "AluOp::Sltu => (a < b) as u32,",
                      "AluOp::Sltu => ((a as i32) < (b as i32)) as u32,")],
        why="SLTU is how unsigned comparisons and carry detection are built.",
    ),
    dict(
        name="Byte loads stop sign-extending",
        edits=[(EXEC, "((raw as u8) as i8) as i32 as u32", "raw")],
        why="LB and LBU differ in exactly this and nothing else.",
    ),
    dict(
        name="Division by zero returns zero instead of all ones",
        edits=[(EXEC, "            if b == 0 {\n                NEG_ONE\n            } else if a == MIN && b == NEG_ONE {",
                      "            if b == 0 {\n                0\n            } else if a == MIN && b == NEG_ONE {")],
        why="The specification fixes this, and it is the case a guest written in "
            "safe Rust can never reach.",
    ),
    dict(
        name="x0 becomes an ordinary register",
        edits=[
            (HART, "        if index == 0 {\n            0\n        } else {\n            self.regs[index]\n        }",
                   "        self.regs[index]"),
            (HART, "        if index != 0 {\n            self.regs[index] = value;\n        }",
                   "        self.regs[index] = value;"),
            (HART, "        let mut out = self.regs;\n        out[0] = 0;\n        out",
                   "        self.regs"),
        ],
        why="x0 reading as zero is the invariant every other instruction assumes. "
            "It is enforced in three places here -- the reader, the writer and the "
            "state dump -- and removing any one of them changes nothing "
            "observable, which is why all three go at once. That redundancy is "
            "real and worth knowing about: no test in this repository pins the "
            "guard in `set_reg` on its own.",
    ),
    dict(
        name="BGE becomes a strict comparison",
        edits=[(EXEC, "BranchOp::Ge => (a as i32) >= (b as i32),",
                      "BranchOp::Ge => (a as i32) > (b as i32),")],
        why="Off by one on the equal case, which is the case loops end on.",
    ),
    dict(
        name="Half-word stores write four bytes",
        edits=[(EXEC, "                StoreOp::H => 2,", "                StoreOp::H => 4,")],
        why="A store that writes more than it should corrupts a neighbour, which "
            "is invisible until something reads it.",
    ),
    # --- shared-misreading mutations: applied to both interpreters at once ---
    dict(
        name="BOTH interpreters and the test encoder misread the S-type immediate",
        edits=[
            (DECODE,
             "    sext(((word >> 25) << 5) | ((word >> 7) & 0x1f), 12)",
             "    sext((word >> 25) | (((word >> 7) & 0x1f) << 7), 12)"),
            (REF,
             "        let imm_s = (((w & 0xfe00_0000) as i32) >> 20) | ((w >> 7) & 0x1f) as i32;",
             "        let imm_s = ((((w >> 25) | (((w >> 7) & 0x1f) << 7)) << 20) as i32) >> 20;"),
            (COMMON,
             "    let hi = (imm >> 5) & 0x7f;\n    let lo = imm & 0x1f;",
             "    let hi = imm & 0x7f;\n    let lo = (imm >> 7) & 0x1f;"),
        ],
        why="The two halves of a store's offset swapped. This is what a misreading "
            "of the specification actually looks like: the decoder, the second "
            "interpreter and the test encoder all agree with each other and all "
            "disagree with the assembler. Only a program encoded outside this "
            "repository can see it, which is the whole argument for the "
            "compiled-program gate.",
    ),
    dict(
        name="BOTH interpreters treat SRAI's shift amount as six bits",
        edits=[
            (DECODE,
             "                0b101 if funct7(word) == 0b0100000 => (AluOp::Sra, (rs2(word) as i32)),",
             "                0b101 if funct7(word) >> 1 == 0b010000 => (AluOp::Sra, ((word >> 20) & 0x3f) as i32),"),
            (REF,
             "                    (0b101, 0b0100000) => ((va as i32) >> sh) as u32,",
             "                    (0b101, f) if f >> 1 == 0b010000 => ((va as i32) >> (((w >> 20) & 0x3f) & 0x1f)) as u32,"),
        ],
        why="An RV64 habit carried into an RV32 decoder. Both interpreters then "
            "accept encodings the specification reserves. This one is expected to "
            "SURVIVE and is kept for that reason: it changes nothing about any "
            "valid program, so no amount of running valid programs can see it, and "
            "the two interpreters agree because both were changed. Catching it "
            "needs a conformance suite that asserts reserved encodings are "
            "refused, which this repository does not have.",
    ),
    dict(
        name="BOTH interpreters compute AUIPC from the following instruction",
        edits=[
            (EXEC, "            hart.set_reg(rd, pc.wrapping_add(imm));",
                   "            hart.set_reg(rd, next.wrapping_add(imm));"),
            (REF, "                self.set(rd, pc.wrapping_add(imm_u));",
                  "                self.set(rd, step4.wrapping_add(imm_u));"),
        ],
        why="Off by one instruction in the base of every pc-relative address a "
            "compiler emits.",
    ),
]


def run_gate(gate):
    proc = subprocess.run(
        ["cargo", "test", "--release", "--test", gate],
        cwd=REPO,
        env=ENV,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        shell=(os.name == "nt"),
    )
    return proc.returncode == 0


def build_ok():
    proc = subprocess.run(
        ["cargo", "build", "--release", "--tests"],
        cwd=REPO,
        env=ENV,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        shell=(os.name == "nt"),
    )
    return proc.returncode == 0


def apply_edits(mutation):
    """Apply every edit, returning (path, original_bytes) pairs for reverting.

    Originals are captured once per file before anything is written, because a
    mutation may edit the same file more than once and saving the file again
    between edits would capture a half-mutated version as the thing to restore.
    """
    saved = {}
    for rel, _old, _new in mutation["edits"]:
        path = os.path.join(REPO, rel.replace("/", os.sep))
        if path not in saved:
            with open(path, "rb") as handle:
                saved[path] = handle.read()

    try:
        for rel, old, new in mutation["edits"]:
            path = os.path.join(REPO, rel.replace("/", os.sep))
            with open(path, "rb") as handle:
                current = handle.read()
            old_b = old.encode("utf-8")
            if current.count(old_b) != 1:
                raise SystemExit(
                    "anchor matched {} times in {} for {!r}".format(
                        current.count(old_b), rel, mutation["name"]
                    )
                )
            with open(path, "wb") as handle:
                handle.write(current.replace(old_b, new.encode("utf-8")))
    except BaseException:
        revert(list(saved.items()))
        raise

    return list(saved.items())


def revert(saved):
    for path, original in saved:
        with open(path, "wb") as handle:
            handle.write(original)


def main():
    print("checking the baseline")
    baseline = {gate: run_gate(gate) for gate in GATES}
    if not all(baseline.values()):
        print("baseline is not green: {}".format(baseline), file=sys.stderr)
        return 1
    print("baseline green on all three gates")

    results = []
    for mutation in MUTATIONS:
        saved = apply_edits(mutation)
        try:
            if not build_ok():
                caught = {gate: True for gate in GATES}
                compiles = False
            else:
                compiles = True
                caught = {gate: not run_gate(gate) for gate in GATES}
        finally:
            revert(saved)
        killed = any(caught.values())
        results.append((mutation, killed, caught, compiles))
        marker = "KILLED  " if killed else "SURVIVED"
        by = ", ".join(g for g in GATES if caught[g]) or "nothing"
        print("{}  {}  [{}]".format(marker, mutation["name"], by))

    print("re-checking the tree")
    if not all(run_gate(gate) for gate in GATES):
        print("restore failed, the tree is not green again", file=sys.stderr)
        return 1
    print("restored, green")

    killed_count = sum(1 for _, k, _, _ in results if k)
    lines = [
        "# Mutation evidence",
        "",
        "A test that has never been observed failing is decoration. Each mutation",
        "below was applied to a green tree, the three gates were run separately, and",
        "the tree was reverted and rebuilt green afterwards.",
        "",
        "Regenerate with `python tools/mutations.py`, which does exactly that and",
        "rewrites this file.",
        "",
        "**{} of {} mutations killed.**".format(killed_count, len(results)),
        "",
        "The last three are applied to *both* interpreters at once. That is what a",
        "misreading of the specification looks like rather than a typo, and the",
        "lockstep comparison is blind to it by construction: two implementations that",
        "are wrong in the same way agree perfectly. Only the compiled-program gate,",
        "where the instructions were selected and encoded by LLVM and the expected",
        "answers come from the same source running on the host, can see them. The",
        "table is the argument for keeping all three gates.",
        "",
        "| Mutation | spec_vectors | lockstep | programs |",
        "| --- | :---: | :---: | :---: |",
    ]
    for mutation, killed, caught, compiles in results:
        cells = " | ".join("caught" if caught[g] else "-" for g in GATES)
        note = "" if compiles else " *(did not compile)*"
        lines.append("| {}{} | {} |".format(mutation["name"], note, cells))
    lines.append("")
    for mutation, killed, caught, compiles in results:
        lines.append("## " + mutation["name"])
        lines.append("")
        lines.append("- **Why it matters:** " + mutation["why"])
        lines.append(
            "- **Verdict:** "
            + ("KILLED" if killed else "SURVIVED, NOT PINNED")
            + (
                ""
                if compiles
                else " (the mutated tree does not compile, so every gate reports it)"
            )
        )
        caught_by = ", ".join("`" + g + "`" for g in GATES if caught[g])
        if killed:
            lines.append("- **Caught by:** " + (caught_by or "the build"))
        lines.append("")
        for rel, old, new in mutation["edits"]:
            lines.append("`{}`".format(rel))
            lines.append("")
            lines.append("```diff")
            for line in old.splitlines():
                lines.append("-" + line)
            for line in new.splitlines():
                lines.append("+" + line)
            lines.append("```")
            lines.append("")

    with open(os.path.join(REPO, "MUTATIONS.md"), "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines))
    print("wrote MUTATIONS.md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
