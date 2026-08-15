//! A second RV32IM interpreter, written differently on purpose.
//!
//! This crate exists to disagree with `hartbeat`. Two implementations only catch
//! anything if their mistakes are uncorrelated, so the differences here are
//! deliberate rather than stylistic:
//!
//! * **No decode step.** `hartbeat` turns a word into a typed `Instr` and then
//!   executes it. This one pulls fields out of the word at the point of use and
//!   matches on `(opcode, funct3, funct7)` in one flat table. A mistake in the
//!   shape of the other crate's enum has nothing to correspond to here.
//! * **Immediates derived differently.** `hartbeat` masks the bits out and then
//!   sign-extends with a shift pair. This one lets an arithmetic shift of the
//!   whole word do the sign extension, and assembles the scattered branch and
//!   jump immediates in a different order.
//! * **Its own memory.** A byte map rather than a page table. Sharing the memory
//!   implementation would have made every memory bug invisible to the
//!   comparison.
//!
//! What it is *not* is independent of the author. Both were written by the same
//! person from the same specification, so a misreading of the specification will
//! appear in both and the comparison will be silent about it. That is the gap
//! `tests/programs.rs` in the other crate is aimed at, where the instructions
//! were chosen and encoded by a compiler instead.

use std::collections::HashMap;

/// Why the reference hart stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefTrap {
    Illegal,
    Misaligned,
    Fault,
    Ecall,
    Ebreak,
}

/// The reference hart.
pub struct RefHart {
    pub x: [u32; 32],
    pub pc: u32,
    mem: HashMap<u32, u8>,
}

impl RefHart {
    pub fn new(pc: u32) -> Self {
        Self {
            x: [0; 32],
            pc,
            mem: HashMap::new(),
        }
    }

    pub fn write_bytes(&mut self, addr: u32, data: &[u8]) {
        for (i, b) in data.iter().enumerate() {
            self.mem.insert(addr.wrapping_add(i as u32), *b);
        }
    }

    /// Read a byte without trapping. For comparing memory after a run.
    pub fn peek(&self, addr: u32) -> Option<u8> {
        self.mem.get(&addr).copied()
    }

    pub fn get(&self, i: u8) -> u32 {
        if i == 0 {
            0
        } else {
            self.x[i as usize]
        }
    }

    pub fn set(&mut self, i: u8, v: u32) {
        if i != 0 {
            self.x[i as usize] = v;
        }
    }

    /// Registers with `x0` forced to zero, for comparison.
    pub fn regs(&self) -> [u32; 32] {
        let mut out = self.x;
        out[0] = 0;
        out
    }

    fn load(&self, addr: u32, width: u32) -> Result<u32, RefTrap> {
        if addr % width != 0 {
            return Err(RefTrap::Misaligned);
        }
        let mut acc = 0u32;
        let mut i = 0;
        while i < width {
            let byte = *self.mem.get(&addr.wrapping_add(i)).ok_or(RefTrap::Fault)?;
            acc += (byte as u32) << (i * 8);
            i += 1;
        }
        Ok(acc)
    }

    fn store(&mut self, addr: u32, value: u32, width: u32) -> Result<(), RefTrap> {
        if addr % width != 0 {
            return Err(RefTrap::Misaligned);
        }
        // Check every byte is mapped before writing any of them, so a partial
        // store cannot happen. The other implementation gets this from its page
        // table; here it has to be explicit.
        let mut i = 0;
        while i < width {
            if !self.mem.contains_key(&addr.wrapping_add(i)) {
                return Err(RefTrap::Fault);
            }
            i += 1;
        }
        let mut i = 0;
        while i < width {
            self.mem
                .insert(addr.wrapping_add(i), (value >> (i * 8)) as u8);
            i += 1;
        }
        Ok(())
    }

    /// One instruction.
    pub fn step(&mut self) -> Result<(), RefTrap> {
        let pc = self.pc;
        if pc % 4 != 0 {
            return Err(RefTrap::Misaligned);
        }
        let w = self.load(pc, 4)?;

        let opcode = w & 0x7f;
        let rd = ((w >> 7) & 0x1f) as u8;
        let f3 = (w >> 12) & 0x7;
        let a = ((w >> 15) & 0x1f) as u8;
        let b = ((w >> 20) & 0x1f) as u8;
        let f7 = w >> 25;

        // Sign extension by arithmetic shift of the whole word.
        let imm_i = (w as i32) >> 20;
        let imm_s = (((w & 0xfe00_0000) as i32) >> 20) | ((w >> 7) & 0x1f) as i32;
        let imm_b = (((w as i32) >> 31) << 12)
            | ((((w >> 7) & 1) as i32) << 11)
            | ((((w >> 25) & 0x3f) as i32) << 5)
            | ((((w >> 8) & 0xf) as i32) << 1);
        let imm_u = w & 0xffff_f000;
        let imm_j = (((w as i32) >> 31) << 20)
            | ((((w >> 12) & 0xff) as i32) << 12)
            | ((((w >> 20) & 1) as i32) << 11)
            | ((((w >> 21) & 0x3ff) as i32) << 1);

        let va = self.get(a);
        let vb = self.get(b);
        let step4 = pc.wrapping_add(4);

        match opcode {
            0b0110111 => {
                self.set(rd, imm_u);
                self.pc = step4;
            }
            0b0010111 => {
                self.set(rd, pc.wrapping_add(imm_u));
                self.pc = step4;
            }
            0b1101111 => {
                self.set(rd, step4);
                self.pc = (pc as i32).wrapping_add(imm_j) as u32;
            }
            0b1100111 => {
                if f3 != 0 {
                    return Err(RefTrap::Illegal);
                }
                let target = ((va as i32).wrapping_add(imm_i) as u32) & 0xffff_fffe;
                self.set(rd, step4);
                self.pc = target;
            }
            0b1100011 => {
                let take = match f3 {
                    0b000 => va == vb,
                    0b001 => va != vb,
                    0b100 => (va as i32) < (vb as i32),
                    0b101 => !((va as i32) < (vb as i32)),
                    0b110 => va < vb,
                    0b111 => !(va < vb),
                    _ => return Err(RefTrap::Illegal),
                };
                self.pc = if take {
                    (pc as i32).wrapping_add(imm_b) as u32
                } else {
                    step4
                };
            }
            0b0000011 => {
                let addr = (va as i32).wrapping_add(imm_i) as u32;
                let v = match f3 {
                    0b000 => {
                        let r = self.load(addr, 1)?;
                        (((r << 24) as i32) >> 24) as u32
                    }
                    0b001 => {
                        let r = self.load(addr, 2)?;
                        (((r << 16) as i32) >> 16) as u32
                    }
                    0b010 => self.load(addr, 4)?,
                    0b100 => self.load(addr, 1)?,
                    0b101 => self.load(addr, 2)?,
                    _ => return Err(RefTrap::Illegal),
                };
                self.set(rd, v);
                self.pc = step4;
            }
            0b0100011 => {
                let addr = (va as i32).wrapping_add(imm_s) as u32;
                let width = match f3 {
                    0b000 => 1,
                    0b001 => 2,
                    0b010 => 4,
                    _ => return Err(RefTrap::Illegal),
                };
                self.store(addr, vb, width)?;
                self.pc = step4;
            }
            0b0010011 => {
                let i = imm_i as u32;
                let sh = (w >> 20) & 0x1f;
                let v = match (f3, f7) {
                    (0b000, _) => va.wrapping_add(i),
                    (0b010, _) => {
                        if (va as i32) < (i as i32) {
                            1
                        } else {
                            0
                        }
                    }
                    (0b011, _) => {
                        if va < i {
                            1
                        } else {
                            0
                        }
                    }
                    (0b100, _) => va ^ i,
                    (0b110, _) => va | i,
                    (0b111, _) => va & i,
                    (0b001, 0b0000000) => va.wrapping_shl(sh),
                    (0b101, 0b0000000) => va.wrapping_shr(sh),
                    (0b101, 0b0100000) => ((va as i32) >> sh) as u32,
                    _ => return Err(RefTrap::Illegal),
                };
                self.set(rd, v);
                self.pc = step4;
            }
            0b0110011 => {
                let sh = vb & 0x1f;
                let v = match (f7, f3) {
                    (0b0000000, 0b000) => va.wrapping_add(vb),
                    (0b0100000, 0b000) => va.wrapping_sub(vb),
                    (0b0000000, 0b001) => va.wrapping_shl(sh),
                    (0b0000000, 0b010) => {
                        if (va as i32) < (vb as i32) {
                            1
                        } else {
                            0
                        }
                    }
                    (0b0000000, 0b011) => {
                        if va < vb {
                            1
                        } else {
                            0
                        }
                    }
                    (0b0000000, 0b100) => va ^ vb,
                    (0b0000000, 0b101) => va.wrapping_shr(sh),
                    (0b0100000, 0b101) => ((va as i32) >> sh) as u32,
                    (0b0000000, 0b110) => va | vb,
                    (0b0000000, 0b111) => va & vb,
                    (0b0000001, f) => mul_div(f, va, vb)?,
                    _ => return Err(RefTrap::Illegal),
                };
                self.set(rd, v);
                self.pc = step4;
            }
            0b0001111 if f3 == 0 => {
                self.pc = step4;
            }
            0b1110011 if f3 == 0 => match w >> 20 {
                0 => return Err(RefTrap::Ecall),
                1 => return Err(RefTrap::Ebreak),
                _ => return Err(RefTrap::Illegal),
            },
            _ => return Err(RefTrap::Illegal),
        }
        Ok(())
    }
}

/// The M extension, computed through 64-bit intermediates throughout so that the
/// high multiplies fall out of one expression each.
fn mul_div(f3: u32, a: u32, b: u32) -> Result<u32, RefTrap> {
    let sa = a as i32 as i64;
    let sb = b as i32 as i64;
    let ua = a as u64;
    let ub = b as u64;
    Ok(match f3 {
        0b000 => (sa.wrapping_mul(sb) as u64 & 0xffff_ffff) as u32,
        0b001 => ((sa * sb) >> 32) as u32,
        0b010 => ((sa * (b as i64)) >> 32) as u32,
        0b011 => ((ua * ub) >> 32) as u32,
        0b100 => {
            if b == 0 {
                u32::MAX
            } else if a == 0x8000_0000 && b == u32::MAX {
                0x8000_0000
            } else {
                ((a as i32) / (b as i32)) as u32
            }
        }
        0b101 => {
            if b == 0 {
                u32::MAX
            } else {
                ((ua / ub) & 0xffff_ffff) as u32
            }
        }
        0b110 => {
            if b == 0 {
                a
            } else if a == 0x8000_0000 && b == u32::MAX {
                0
            } else {
                ((a as i32) % (b as i32)) as u32
            }
        }
        0b111 => {
            if b == 0 {
                a
            } else {
                ((ua % ub) & 0xffff_ffff) as u32
            }
        }
        _ => return Err(RefTrap::Illegal),
    })
}
