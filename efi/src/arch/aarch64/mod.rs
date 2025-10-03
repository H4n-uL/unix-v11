use core::arch::asm;

pub fn set_interrupts(enabled: bool) {
    unsafe {
        if enabled {
            asm!("msr daifclr, 0b1111");
        } else {
            asm!("msr daifset, 0b1111");
        }
    }
}

pub fn halt() {
    set_interrupts(false);
    unsafe { asm!("wfi"); }
}

pub const R_RELATIVE: u64 = 1027;

#[inline(always)]
pub fn stack_ptr() -> usize {
    let sp: usize;
    unsafe { asm!("mov {}, sp", out(reg) sp); }
    return sp;
}
