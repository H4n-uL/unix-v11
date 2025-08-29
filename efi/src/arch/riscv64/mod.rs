pub fn set_interrupts(enabled: bool) {
    unsafe {
        let mut mstatus: usize;
        core::arch::asm!("csrr {}, mstatus", out(reg) mstatus);
        if enabled { mstatus |= 0x8; } else { mstatus &= !0x8; }
        core::arch::asm!("csrw mstatus, {}", in(reg) mstatus);
    }
}

pub fn halt() {
    set_interrupts(false);
    unsafe { core::arch::asm!("wfi"); }
}

pub const R_RELATIVE: u64 = 3;

#[inline(always)]
pub fn stack_ptr() -> usize {
    let sp: usize;
    unsafe { core::arch::asm!("mv {}, sp", out(reg) sp); }
    return sp;
}