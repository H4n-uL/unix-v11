pub mod exc;
pub mod intc;
pub mod proc;
pub mod rvm;

use crate::{
    arch::rvm::flags,
    ram::glacier::{GLACIER, page_size}
};

use core::{arch::asm, fmt::{Result, Write}, hint::spin_loop};

pub fn wfi() {
    exc::set(true);
    unsafe { asm!("wfi"); }
}

pub fn halt() {
    exc::set(false);
    unsafe { asm!("wfi"); }
}

pub const R_REL: usize    = 1027; // R_RELATIVE
pub const R_SYM: &[usize] = &[
    257,  // R_64:        S + A
    1025, // R_GLOB_DAT:  S
    1026  // R_JUMP_SLOT: S
];

const UART0_BASE: usize = 0x0900_0000; // QEMU virt PL011 UART

#[inline(always)]
fn serial_io() -> usize {
    0usize.wrapping_sub(page_size())
}

#[inline(always)]
pub fn phys_id() -> usize {
    let mpidr: usize;
    unsafe { asm!("mrs {}, mpidr_el1", out(reg) mpidr); }
    return mpidr & 0xffff;
}

pub fn init_serial() {
    let sio = serial_io();
    GLACIER.write().map_page(sio, UART0_BASE, flags::D_RW);

    unsafe {
        // Disable UART
        ((sio + 0x30) as *mut u32).write_volatile(0x0);
        // Clear all pending interrupts
        ((sio + 0x44) as *mut u32).write_volatile(0x7ff);
        // Enable UART, TX, RX
        ((sio + 0x30) as *mut u32).write_volatile(0x301); // UARTCR: UARTEN|TXE|RXE
    }
}

pub fn serial_putchar(c: u8) {
    let sio = serial_io();
    unsafe {
        while ((sio + 0x18) as *const u32).read_volatile() & (1 << 5) != 0 { spin_loop(); }
        ((sio + 0x00) as *mut u32).write_volatile(c as u32);
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

#[inline(always)]
pub unsafe fn move_stack(addr: usize) {
    unsafe {
        asm!("mov sp, {}", in(reg) addr);
    }
}
