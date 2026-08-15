# Mutation evidence

A test that has never been observed failing is decoration. Each mutation
below was applied to a green tree, the three gates were run separately, and
the tree was reverted and rebuilt green afterwards.

Regenerate with `python tools/mutations.py`, which does exactly that and
rewrites this file.

**11 of 12 mutations killed.**

The last three are applied to *both* interpreters at once. That is what a
misreading of the specification looks like rather than a typo, and the
lockstep comparison is blind to it by construction: two implementations that
are wrong in the same way agree perfectly. Only the compiled-program gate,
where the instructions were selected and encoded by LLVM and the expected
answers come from the same source running on the host, can see them. The
table is the argument for keeping all three gates.

| Mutation | spec_vectors | lockstep | programs |
| --- | :---: | :---: | :---: |
| Arithmetic right shift becomes logical | caught | caught | caught |
| A shift of 32 or more produces zero instead of wrapping | caught | caught | caught |
| JALR stops clearing the low bit of its target | caught | - | - |
| Unsigned comparison becomes signed | caught | caught | caught |
| Byte loads stop sign-extending | caught | caught | caught |
| Division by zero returns zero instead of all ones | caught | caught | caught |
| x0 becomes an ordinary register | caught | caught | caught |
| BGE becomes a strict comparison | caught | caught | - |
| Half-word stores write four bytes | - | caught | caught |
| BOTH interpreters and the test encoder misread the S-type immediate | - | - | caught |
| BOTH interpreters treat SRAI's shift amount as six bits | - | - | - |
| BOTH interpreters compute AUIPC from the following instruction | caught | - | caught |

## Arithmetic right shift becomes logical

- **Why it matters:** The difference only shows on negative values, which is most of the interesting ones.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `lockstep`, `programs`

`crates/hartbeat/src/exec.rs`

```diff
-AluOp::Sra => ((a as i32) >> (b & 0x1f)) as u32,
+AluOp::Sra => a >> (b & 0x1f),
```

## A shift of 32 or more produces zero instead of wrapping

- **Why it matters:** Shifting by 33 must shift by one. Note that the obvious mutation -- replacing the mask with `wrapping_shl` -- is not a mutation at all, because Rust's `wrapping_shl` masks to the width itself. That one was tried first and survived, for that reason and not because anything was untested.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `lockstep`, `programs`

`crates/hartbeat/src/exec.rs`

```diff
-AluOp::Sll => a << (b & 0x1f),
+AluOp::Sll => a.checked_shl(b).unwrap_or(0),
```

## JALR stops clearing the low bit of its target

- **Why it matters:** Every indirect call goes through this, and the bit is only ever set when the offset is odd -- which a compiler does emit.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`

`crates/hartbeat/src/exec.rs`

```diff
-let target = hart.reg(rs1).wrapping_add(imm as u32) & !1;
+let target = hart.reg(rs1).wrapping_add(imm as u32);
```

## Unsigned comparison becomes signed

- **Why it matters:** SLTU is how unsigned comparisons and carry detection are built.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `lockstep`, `programs`

`crates/hartbeat/src/exec.rs`

```diff
-AluOp::Sltu => (a < b) as u32,
+AluOp::Sltu => ((a as i32) < (b as i32)) as u32,
```

## Byte loads stop sign-extending

- **Why it matters:** LB and LBU differ in exactly this and nothing else.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `lockstep`, `programs`

`crates/hartbeat/src/exec.rs`

```diff
-((raw as u8) as i8) as i32 as u32
+raw
```

## Division by zero returns zero instead of all ones

- **Why it matters:** The specification fixes this, and it is the case a guest written in safe Rust can never reach.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `lockstep`, `programs`

`crates/hartbeat/src/exec.rs`

```diff
-            if b == 0 {
-                NEG_ONE
-            } else if a == MIN && b == NEG_ONE {
+            if b == 0 {
+                0
+            } else if a == MIN && b == NEG_ONE {
```

## x0 becomes an ordinary register

- **Why it matters:** x0 reading as zero is the invariant every other instruction assumes. It is enforced in three places here -- the reader, the writer and the state dump -- and removing any one of them changes nothing observable, which is why all three go at once. That redundancy is real and worth knowing about: no test in this repository pins the guard in `set_reg` on its own.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `lockstep`, `programs`

`crates/hartbeat/src/hart.rs`

```diff
-        if index == 0 {
-            0
-        } else {
-            self.regs[index]
-        }
+        self.regs[index]
```

`crates/hartbeat/src/hart.rs`

```diff
-        if index != 0 {
-            self.regs[index] = value;
-        }
+        self.regs[index] = value;
```

`crates/hartbeat/src/hart.rs`

```diff
-        let mut out = self.regs;
-        out[0] = 0;
-        out
+        self.regs
```

## BGE becomes a strict comparison

- **Why it matters:** Off by one on the equal case, which is the case loops end on.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `lockstep`

`crates/hartbeat/src/exec.rs`

```diff
-BranchOp::Ge => (a as i32) >= (b as i32),
+BranchOp::Ge => (a as i32) > (b as i32),
```

## Half-word stores write four bytes

- **Why it matters:** A store that writes more than it should corrupts a neighbour, which is invisible until something reads it.
- **Verdict:** KILLED
- **Caught by:** `lockstep`, `programs`

`crates/hartbeat/src/exec.rs`

```diff
-                StoreOp::H => 2,
+                StoreOp::H => 4,
```

## BOTH interpreters and the test encoder misread the S-type immediate

- **Why it matters:** The two halves of a store's offset swapped. This is what a misreading of the specification actually looks like: the decoder, the second interpreter and the test encoder all agree with each other and all disagree with the assembler. Only a program encoded outside this repository can see it, which is the whole argument for the compiled-program gate.
- **Verdict:** KILLED
- **Caught by:** `programs`

`crates/hartbeat/src/decode.rs`

```diff
-    sext(((word >> 25) << 5) | ((word >> 7) & 0x1f), 12)
+    sext((word >> 25) | (((word >> 7) & 0x1f) << 7), 12)
```

`crates/hartbeat-ref/src/lib.rs`

```diff
-        let imm_s = (((w & 0xfe00_0000) as i32) >> 20) | ((w >> 7) & 0x1f) as i32;
+        let imm_s = ((((w >> 25) | (((w >> 7) & 0x1f) << 7)) << 20) as i32) >> 20;
```

`crates/hartbeat/tests/common/mod.rs`

```diff
-    let hi = (imm >> 5) & 0x7f;
-    let lo = imm & 0x1f;
+    let hi = imm & 0x7f;
+    let lo = (imm >> 7) & 0x1f;
```

## BOTH interpreters treat SRAI's shift amount as six bits

- **Why it matters:** An RV64 habit carried into an RV32 decoder. Both interpreters then accept encodings the specification reserves. This one is expected to SURVIVE and is kept for that reason: it changes nothing about any valid program, so no amount of running valid programs can see it, and the two interpreters agree because both were changed. Catching it needs a conformance suite that asserts reserved encodings are refused, which this repository does not have.
- **Verdict:** SURVIVED, NOT PINNED

`crates/hartbeat/src/decode.rs`

```diff
-                0b101 if funct7(word) == 0b0100000 => (AluOp::Sra, (rs2(word) as i32)),
+                0b101 if funct7(word) >> 1 == 0b010000 => (AluOp::Sra, ((word >> 20) & 0x3f) as i32),
```

`crates/hartbeat-ref/src/lib.rs`

```diff
-                    (0b101, 0b0100000) => ((va as i32) >> sh) as u32,
+                    (0b101, f) if f >> 1 == 0b010000 => ((va as i32) >> (((w >> 20) & 0x3f) & 0x1f)) as u32,
```

## BOTH interpreters compute AUIPC from the following instruction

- **Why it matters:** Off by one instruction in the base of every pc-relative address a compiler emits.
- **Verdict:** KILLED
- **Caught by:** `spec_vectors`, `programs`

`crates/hartbeat/src/exec.rs`

```diff
-            hart.set_reg(rd, pc.wrapping_add(imm));
+            hart.set_reg(rd, next.wrapping_add(imm));
```

`crates/hartbeat-ref/src/lib.rs`

```diff
-                self.set(rd, pc.wrapping_add(imm_u));
+                self.set(rd, step4.wrapping_add(imm_u));
```
