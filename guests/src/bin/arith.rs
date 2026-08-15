//! Integer arithmetic and logic over two volatile inputs.
#![no_std]
#![no_main]

include!("../runtime.rs");
include!("../algorithms.rs");

#[no_mangle]
pub extern "C" fn guest_main() {
    let mut out = [0u32; 20];
    arith(input(0), input(1), &mut out);
    for (i, value) in out.iter().enumerate() {
        output(i, *value);
    }
}
