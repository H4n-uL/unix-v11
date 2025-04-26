use x86_64::instructions::{hlt, interrupts};

pub fn halt() {
    interrupts::disable();
    hlt();
}

#[inline(always)]
pub fn stack_ptr() -> usize {
    let rsp: usize;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp); }
    return rsp;
}