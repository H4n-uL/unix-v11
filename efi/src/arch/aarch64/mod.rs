pub fn set_interrupts(enabled: bool) {
    unsafe {
        if enabled {
            core::arch::asm!("msr daifclr, 0b1111");
        } else {
            core::arch::asm!("msr daifset, 0b1111");
        }
    }
}

pub fn halt() {
    set_interrupts(false);
    unsafe { core::arch::asm!("wfi"); }
}

pub const R_RELATIVE: u64 = 1027;

#[inline(always)]
pub fn stack_ptr() -> usize {
    let sp: usize;
    unsafe { core::arch::asm!("mov {}, sp", out(reg) sp); }
    return sp;
}
