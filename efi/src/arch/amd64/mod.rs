use core::arch::asm;

pub fn set_interrupts(enabled: bool) {
    unsafe {
        if enabled {
            asm!("sti");
        } else {
            asm!("cli");
        }
    }
}

pub fn halt() {
    set_interrupts(false);
    unsafe { asm!("hlt"); }
}

pub const R_RELATIVE: u64 = 8;

#[inline(always)]
pub fn stack_ptr() -> usize {
    let rsp: usize;
    unsafe { asm!("mov {}, rsp", out(reg) rsp); }
    return rsp;
}
