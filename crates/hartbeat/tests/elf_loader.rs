//! What the loader refuses.
//!
//! Every other test here hands the loader an image the toolchain produced, so
//! the only path with coverage is the one where everything is right. The
//! refusals are the interesting half: the loader decides what bytes become the
//! program, and a file it accepts without loading anything from is a program the
//! emulator will then blame for faulting at its own entry point.

use hartbeat::{elf, Memory};

/// A 52-byte ELF32 header the loader accepts, with the program header table
/// described by the caller. Written by hand rather than compiled, because the
/// point is a file the toolchain would never emit.
fn header(e_type: u16, entry: u32, phoff: u32, phentsize: u16, phnum: u16) -> Vec<u8> {
    let mut b = vec![0u8; 52];
    b[0..4].copy_from_slice(b"\x7fELF");
    b[4] = 1; // ELFCLASS32
    b[5] = 1; // little-endian
    b[6] = 1; // EV_CURRENT
    b[16..18].copy_from_slice(&e_type.to_le_bytes());
    b[18..20].copy_from_slice(&0xf3u16.to_le_bytes()); // EM_RISCV
    b[24..28].copy_from_slice(&entry.to_le_bytes());
    b[28..32].copy_from_slice(&phoff.to_le_bytes());
    b[42..44].copy_from_slice(&phentsize.to_le_bytes());
    b[44..46].copy_from_slice(&phnum.to_le_bytes());
    b
}

/// An object file is an ELF32 RISC-V file with no program headers at all, and a
/// linked image can carry headers that are not `PT_LOAD`. Neither is a program.
/// Accepting one leaves the hart at an entry point nothing was loaded at, and
/// the fault that follows reads as the program's fault rather than the loader's.
#[test]
fn an_image_that_loads_nothing_is_refused() {
    // ET_REL, no program header table: what `rustc --emit obj` leaves behind.
    let object = header(1, 0, 0, 0, 0);
    let mut mem = Memory::new();
    assert_eq!(
        elf::load(&object, &mut mem),
        Err(elf::ElfError::NothingToLoad),
        "an object file is not a program"
    );
    assert_eq!(mem.mapped_pages(), 0, "nothing should have been mapped");

    // One program header, and it is PT_NOTE rather than PT_LOAD.
    let mut note = header(2, 0x8000_0000, 52, 32, 1);
    note.extend_from_slice(&4u32.to_le_bytes()); // p_type = PT_NOTE
    note.extend_from_slice(&[0u8; 28]);
    let mut mem = Memory::new();
    assert_eq!(
        elf::load(&note, &mut mem),
        Err(elf::ElfError::NothingToLoad),
        "a file whose only segment is not loadable is not a program"
    );
}
