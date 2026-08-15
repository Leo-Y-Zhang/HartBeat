//! A minimal ELF32 loader: enough to load what the Rust toolchain emits for
//! `riscv32im-unknown-none-elf`, and nothing more.
//!
//! Unverified in the same sense as any parser: it decides what bytes become the
//! program, so a bug here changes what is executed. It refuses anything it does
//! not fully understand rather than guessing.

use crate::mem::Memory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElfError {
    NotElf,
    Not32Bit,
    NotLittleEndian,
    NotRiscV,
    Truncated,
}

impl std::fmt::Display for ElfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            ElfError::NotElf => "not an ELF file",
            ElfError::Not32Bit => "not a 32-bit ELF",
            ElfError::NotLittleEndian => "not little-endian",
            ElfError::NotRiscV => "not a RISC-V ELF",
            ElfError::Truncated => "truncated",
        };
        f.write_str(text)
    }
}

const EM_RISCV: u16 = 0xf3;
const PT_LOAD: u32 = 1;

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, ElfError> {
    bytes
        .get(offset..offset + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(ElfError::Truncated)
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, ElfError> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(ElfError::Truncated)
}

/// Load every `PT_LOAD` segment into `mem` and return the entry point.
///
/// A segment whose `memsz` exceeds its `filesz` has the remainder zeroed, which
/// is how `.bss` arrives.
pub fn load(bytes: &[u8], mem: &mut Memory) -> Result<u32, ElfError> {
    if bytes.len() < 52 || &bytes[0..4] != b"\x7fELF" {
        return Err(ElfError::NotElf);
    }
    if bytes[4] != 1 {
        return Err(ElfError::Not32Bit);
    }
    if bytes[5] != 1 {
        return Err(ElfError::NotLittleEndian);
    }
    if u16_at(bytes, 18)? != EM_RISCV {
        return Err(ElfError::NotRiscV);
    }

    let entry = u32_at(bytes, 24)?;
    let phoff = u32_at(bytes, 28)? as usize;
    let phentsize = u16_at(bytes, 42)? as usize;
    let phnum = u16_at(bytes, 44)? as usize;

    for i in 0..phnum {
        let base = phoff + i * phentsize;
        if u32_at(bytes, base)? != PT_LOAD {
            continue;
        }
        let offset = u32_at(bytes, base + 4)? as usize;
        let vaddr = u32_at(bytes, base + 8)?;
        let filesz = u32_at(bytes, base + 16)? as usize;
        let memsz = u32_at(bytes, base + 20)? as usize;

        let data = bytes
            .get(offset..offset + filesz)
            .ok_or(ElfError::Truncated)?;
        mem.write_slice(vaddr, data);

        // Zero-fill to memsz, and make sure every page of the segment exists
        // even when it is entirely .bss.
        if memsz > filesz {
            let zeros = vec![0u8; memsz - filesz];
            mem.write_slice(vaddr.wrapping_add(filesz as u32), &zeros);
        }
    }

    Ok(entry)
}
