use aarch64_cpu::{asm::wfi, registers::DAIF};
use tock_registers::interfaces::{Readable, Writeable};

pub fn halt() {
    DAIF.set(DAIF.get() | 0b1111);
    wfi();
}

#[inline(always)]
pub fn stack_ptr() -> usize {
    let sp: usize;
    unsafe { core::arch::asm!("mov {}, sp", out(reg) sp); }
    return sp;
}