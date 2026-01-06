pub mod inter; pub mod rvm;

use crate::{
    arch::rvm::flags,
    ram::{PAGE_4KIB, glacier::GLACIER}
};

use core::{arch::asm, fmt::{Result, Write}, hint::spin_loop};

pub fn halt() {
    inter::set(false);
    unsafe { asm!("wfi"); }
}

pub const R_RELATIVE: usize = 1027;
const SERIAL_IO: usize = 0usize.wrapping_sub(PAGE_4KIB);
const UART0_BASE: usize = 0x0900_0000; // QEMU virt PL011 UART

pub fn init_serial() {
    GLACIER.write().map_page(
        SERIAL_IO, UART0_BASE, flags::D_RW
    );

    unsafe {
        // Disable UART
        ((SERIAL_IO + 0x30) as *mut u32).write_volatile(0x0);
        // Clear all pending interrupts
        ((SERIAL_IO + 0x44) as *mut u32).write_volatile(0x7ff);
        // Enable UART, TX, RX
        ((SERIAL_IO + 0x30) as *mut u32).write_volatile(0x301); // UARTCR: UARTEN|TXE|RXE
    }
}

pub fn serial_putchar(c: u8) {
    unsafe {
        while ((SERIAL_IO + 0x18) as *const u32).read_volatile() & (1 << 5) != 0 { spin_loop(); }
        ((SERIAL_IO + 0x00) as *mut u32).write_volatile(c as u32);
    }
}

pub struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> Result {
        for byte in s.bytes() { serial_putchar(byte); }
        Ok(())
    }
}

#[inline(always)]
pub fn stack_ptr() -> *const u8 {
    let sp: usize;
    unsafe { asm!("mov {}, sp", out(reg) sp); }
    return sp as *const u8;
}

// ALL STACK VARIABLES ARE VOID BEYOND THIS POINT.
#[inline(always)]
pub unsafe fn move_stack(addr: usize, size: usize) {
    unsafe {
        (addr as *mut u8).write_bytes(0, size);
        asm!("mov sp, {}", in(reg) addr + size);
    }
}
