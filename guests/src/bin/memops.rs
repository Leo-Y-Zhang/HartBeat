//! Loads and stores at every width, and the sign extension that goes with them.
#![no_std]
#![no_main]

include!("../runtime.rs");

const SCRATCH: usize = OUT_BASE + 256;

#[no_mangle]
pub extern "C" fn guest_main() {
    let seed = input(0);

    unsafe {
        // Four bytes, written one at a time.
        core::ptr::write_volatile(SCRATCH as *mut u8, seed as u8);
        core::ptr::write_volatile((SCRATCH + 1) as *mut u8, (seed >> 8) as u8);
        core::ptr::write_volatile((SCRATCH + 2) as *mut u8, (seed >> 16) as u8);
        core::ptr::write_volatile((SCRATCH + 3) as *mut u8, (seed >> 24) as u8);

        // Read them back at three widths, signed and unsigned.
        output(0, core::ptr::read_volatile(SCRATCH as *const i8) as i32 as u32);
        output(1, core::ptr::read_volatile(SCRATCH as *const u8) as u32);
        output(2, core::ptr::read_volatile(SCRATCH as *const i16) as i32 as u32);
        output(3, core::ptr::read_volatile(SCRATCH as *const u16) as u32);
        output(4, core::ptr::read_volatile(SCRATCH as *const u32));

        // A half-word store must leave the neighbouring half alone.
        core::ptr::write_volatile(SCRATCH as *mut u32, 0xaaaa_aaaa);
        core::ptr::write_volatile(SCRATCH as *mut u16, seed as u16);
        output(5, core::ptr::read_volatile(SCRATCH as *const u32));

        // And a byte store must leave the other three alone.
        core::ptr::write_volatile(SCRATCH as *mut u32, 0x5555_5555);
        core::ptr::write_volatile((SCRATCH + 2) as *mut u8, (seed >> 8) as u8);
        output(6, core::ptr::read_volatile(SCRATCH as *const u32));

        // Unaligned within the word, but aligned for its width.
        core::ptr::write_volatile((SCRATCH + 2) as *mut u16, (seed >> 3) as u16);
        output(7, core::ptr::read_volatile((SCRATCH + 2) as *const u16) as u32);
    }
}
