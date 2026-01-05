use core::arch::asm;

pub const R_RELATIVE: usize = 1027;

pub fn halt() {
    unsafe { asm!("msr daifset, 0b1111", "wfi"); }
}
