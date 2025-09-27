/* pub mod inter; */ pub mod mmu;

use crate::{
    arch::mmu::flags,
    ram::{glacier::GLACIER, physalloc::OwnedPtr}
};

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
const UART0_BASE: usize = 0x0900_0000; // QEMU virt PL011 UART

pub fn init_serial() {
    GLACIER.map_page(0x0900_0000, 0x0900_0000, flags::D_RW);
    GLACIER.map_page(0x0800_0000, 0x0800_0000, flags::D_RW);
    GLACIER.map_page(0x0801_0000, 0x0801_0000, flags::D_RW);
    unsafe {
        // Disable UART
        core::ptr::write_volatile((UART0_BASE + 0x30) as *mut u32, 0x0);
        // Clear all pending interrupts
        core::ptr::write_volatile((UART0_BASE + 0x44) as *mut u32, 0x7ff);
        // Enable UART, TX, RX
        core::ptr::write_volatile((UART0_BASE + 0x30) as *mut u32, 0x301); // UARTCR: UARTEN|TXE|RXE
    }
}

pub fn serial_putchar(c: u8) {
    unsafe {
        while core::ptr::read_volatile((UART0_BASE + 0x18) as *const u32) & (1 << 5) != 0 { core::hint::spin_loop(); }
        core::ptr::write_volatile((UART0_BASE + 0x00) as *mut u32, c as u32);
    }
}

pub struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() { serial_putchar(byte); }
        Ok(())
    }
}

#[inline(always)]
pub fn stack_ptr() -> *const u8 {
    let sp: usize;
    unsafe { core::arch::asm!("mov {}, sp", out(reg) sp); }
    return sp as *const u8;
}

// ALL STACK VARIABLES ARE VOID BEYOND THIS POINT.
#[inline(always)]
pub unsafe fn move_stack(ptr: &OwnedPtr) {
    unsafe {
        core::ptr::write_bytes(ptr.ptr::<u8>(), 0, ptr.size());
        core::arch::asm!("mov sp, {}", in(reg) ptr.addr() + ptr.size());
    }
}
