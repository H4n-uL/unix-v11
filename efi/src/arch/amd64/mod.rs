use core::arch::asm;

pub const R_REL: usize    = 8; // R_RELATIVE
pub const R_SYM: &[usize] = &[
    1, // R_64:        S + A
    6, // R_GLOB_DAT:  S
    7  // R_JUMP_SLOT: S
];

pub fn halt() {
    unsafe { asm!("cli", "hlt"); }
}
