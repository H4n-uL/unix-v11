pub fn set_interrupts(enabled: bool) {
    unsafe {
        if enabled { core::arch::asm!("sti"); }
        else { core::arch::asm!("cli"); }
    }
}

pub fn halt() {
    set_interrupts(false);
    unsafe { core::arch::asm!("hlt"); }
}

#[inline(always)]
pub fn stack_ptr() -> usize {
    let rsp: usize;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp); }
    return rsp;
}