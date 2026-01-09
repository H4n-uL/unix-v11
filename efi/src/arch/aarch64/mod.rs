use core::arch::asm;

pub const R_REL: usize    = 1027; // R_RELATIVE
pub const R_SYM: &[usize] = &[
    257,  // R_64:        S + A
    1025, // R_GLOB_DAT:  S
    1026  // R_JUMP_SLOT: S
];

pub fn halt() {
    unsafe { asm!("msr daifset, 0b1111", "wfi"); }
}
