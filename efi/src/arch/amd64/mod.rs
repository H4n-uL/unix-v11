use core::arch::asm;

pub const R_RELATIVE: usize = 8;

pub fn halt() {
    unsafe { asm!("cli", "hlt"); }
}
