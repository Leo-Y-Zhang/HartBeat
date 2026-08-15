//! Insertion sort over 32 words read from the input page.
//!
//! A real algorithm rather than a list of operations: nested loops, a
//! data-dependent branch, and loads and stores through computed addresses, all
//! interacting. A single wrong comparison changes the whole output rather than
//! one word of it.
#![no_std]
#![no_main]

include!("../runtime.rs");
include!("../algorithms.rs");

#[no_mangle]
pub extern "C" fn guest_main() {
    let mut values = [0u32; 32];
    for (i, slot) in values.iter_mut().enumerate() {
        *slot = input(i);
    }
    let checksum = sort_and_checksum(&mut values);
    for (i, value) in values.iter().enumerate() {
        output(i, *value);
    }
    output(32, checksum);
}
