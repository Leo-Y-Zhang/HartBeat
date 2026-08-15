//! The M extension, including the cases Rust will not let you write.
//!
//! `a / 0` and `i32::MIN / -1` both panic in Rust, so a guest written in safe
//! Rust can never execute them -- and those are exactly the two cases where the
//! ISA's answer is unusual. The instructions are emitted directly instead, which
//! also means the assembler chose the encoding rather than this project.
#![no_std]
#![no_main]

include!("../runtime.rs");

macro_rules! rv {
    ($op:literal, $a:expr, $b:expr) => {{
        let out: u32;
        unsafe {
            core::arch::asm!(
                concat!($op, " {0}, {1}, {2}"),
                out(reg) out,
                in(reg) $a,
                in(reg) $b,
                options(pure, nomem, nostack),
            );
        }
        out
    }};
}

#[no_mangle]
pub extern "C" fn guest_main() {
    let a = input(0);
    let b = input(1);

    output(0, rv!("mul", a, b));
    output(1, rv!("mulh", a, b));
    output(2, rv!("mulhsu", a, b));
    output(3, rv!("mulhu", a, b));
    output(4, rv!("div", a, b));
    output(5, rv!("divu", a, b));
    output(6, rv!("rem", a, b));
    output(7, rv!("remu", a, b));

    // The two specified special cases, reached whatever the inputs are.
    output(8, rv!("div", a, 0u32));
    output(9, rv!("divu", a, 0u32));
    output(10, rv!("rem", a, 0u32));
    output(11, rv!("remu", a, 0u32));
    output(12, rv!("div", 0x8000_0000u32, 0xffff_ffffu32));
    output(13, rv!("rem", 0x8000_0000u32, 0xffff_ffffu32));
}
