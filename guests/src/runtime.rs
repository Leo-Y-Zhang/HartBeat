// Shared guest runtime. Included by each program with `include!`, because these
// are freestanding binaries and a shared crate would put a linkage question
// between the test and the thing being tested.
//
// The contract with the emulator is four things: entry at 0x80000000, inputs at
// 0x80010000, outputs at 0x80011000, and `ebreak` to say "done".

core::arch::global_asm!(
    ".section .text._start",
    ".globl _start",
    "_start:",
    "lui sp, %hi(_stack_top)",
    "addi sp, sp, %lo(_stack_top)",
    "call guest_main",
    "ebreak",
    "1: j 1b",
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // A panicking guest must not look like a finished one, so this does not
    // reach the `ebreak`. The emulator's step budget ends the run and the test
    // sees a step-limit stop rather than a halt.
    loop {}
}

const IN_BASE: usize = 0x8001_0000;
const OUT_BASE: usize = 0x8001_1000;

/// Volatile so the compiler cannot fold the program into its answer. Without
/// this every one of these guests compiles to a handful of stores of constants,
/// which exercises nothing.
#[inline(always)]
fn input(i: usize) -> u32 {
    unsafe { core::ptr::read_volatile((IN_BASE + i * 4) as *const u32) }
}

#[inline(always)]
fn output(i: usize, v: u32) {
    unsafe { core::ptr::write_volatile((OUT_BASE + i * 4) as *mut u32, v) }
}
