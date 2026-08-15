//! Sparse memory.
//!
//! A 32-bit address space is four gigabytes and a guest touches a few pages of
//! it, so pages are allocated on first write and a read of an untouched page is
//! an access fault rather than a zero. That choice is deliberate: a guest that
//! reads uninitialised memory has a bug, and silently returning zero would hide
//! it behind plausible-looking output.

use std::collections::BTreeMap;

use crate::trap::Trap;

/// Page size. Nothing depends on the value beyond allocation granularity.
pub const PAGE_BITS: u32 = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_BITS;

/// Byte-addressed memory, allocated a page at a time.
#[derive(Clone, Default)]
pub struct Memory {
    pages: BTreeMap<u32, Box<[u8; PAGE_SIZE]>>,
}

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }

    fn page_of(addr: u32) -> u32 {
        addr >> PAGE_BITS
    }

    fn offset_of(addr: u32) -> usize {
        (addr & (PAGE_SIZE as u32 - 1)) as usize
    }

    /// Make the page containing `addr` exist.
    pub fn touch(&mut self, addr: u32) {
        self.pages
            .entry(Self::page_of(addr))
            .or_insert_with(|| Box::new([0u8; PAGE_SIZE]));
    }

    /// Place bytes at `addr`, allocating as needed. Used by the loader and by
    /// tests; not reachable from guest code.
    pub fn write_slice(&mut self, addr: u32, data: &[u8]) {
        for (i, byte) in data.iter().enumerate() {
            let a = addr.wrapping_add(i as u32);
            self.touch(a);
            let page = self.pages.get_mut(&Self::page_of(a)).expect("just touched");
            page[Self::offset_of(a)] = *byte;
        }
    }

    /// Whether the page containing `addr` has been allocated.
    pub fn is_mapped(&self, addr: u32) -> bool {
        self.pages.contains_key(&Self::page_of(addr))
    }

    fn read_byte(&self, addr: u32, pc: u32) -> Result<u8, Trap> {
        self.pages
            .get(&Self::page_of(addr))
            .map(|page| page[Self::offset_of(addr)])
            .ok_or(Trap::AccessFault { pc, addr })
    }

    fn write_byte(&mut self, addr: u32, value: u8, pc: u32) -> Result<(), Trap> {
        let page = self
            .pages
            .get_mut(&Self::page_of(addr))
            .ok_or(Trap::AccessFault { pc, addr })?;
        page[Self::offset_of(addr)] = value;
        Ok(())
    }

    /// Read `width` bytes little-endian. `width` is 1, 2 or 4.
    pub fn read(&self, addr: u32, width: u32, pc: u32) -> Result<u32, Trap> {
        if addr % width != 0 {
            return Err(Trap::MisalignedAccess { pc, addr, width });
        }
        let mut value: u32 = 0;
        for i in 0..width {
            let byte = self.read_byte(addr.wrapping_add(i), pc)?;
            value |= (byte as u32) << (8 * i);
        }
        Ok(value)
    }

    /// Write the low `width` bytes of `value` little-endian.
    pub fn write(&mut self, addr: u32, value: u32, width: u32, pc: u32) -> Result<(), Trap> {
        if addr % width != 0 {
            return Err(Trap::MisalignedAccess { pc, addr, width });
        }
        for i in 0..width {
            let byte = (value >> (8 * i)) as u8;
            self.write_byte(addr.wrapping_add(i), byte, pc)?;
        }
        Ok(())
    }

    /// Number of allocated pages. Reported by the CLI, and a cheap way for a
    /// test to notice an unintended allocation.
    pub fn mapped_pages(&self) -> usize {
        self.pages.len()
    }
}
