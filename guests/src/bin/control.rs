//! Data-dependent control flow.
#![no_std]
#![no_main]

include!("../runtime.rs");
include!("../algorithms.rs");

#[no_mangle]
pub extern "C" fn guest_main() {
    let mut out = [0u32; 6];
    control(input(0), &mut out);
    for (i, value) in out.iter().enumerate() {
        output(i, *value);
    }
}
