pub mod inter; pub mod mmu;

use core::{arch::asm, fmt::{Result, Write}};

use crate::ram::physalloc::OwnedPtr;

pub fn halt() {
    inter::set(false);
    unsafe { asm!("hlt"); }
}

pub const R_RELATIVE: u64 = 8;
const COM1: u16 = 0x3f8;

pub fn init_serial() {
    unsafe {
        asm!(
            "mov dx, {com1_base}",
            "inc dx",       // COM1 + 1
            "mov al, 0x00",
            "out dx, al",   // Disable all interrupts

            "add dx, 2",    // COM1 + 3
            "mov al, 0x80",
            "out dx, al",   // Enable DLAB (set baud rate divisor)

            "sub dx, 3",    // COM1 + 0
            "mov al, 0x01",
            "out dx, al",   // Set divisor to 1 (lo byte) 115200 baud

            "inc dx",       // COM1 + 1
            "mov al, 0x00",
            "out dx, al",   //                  (hi byte)

            "add dx, 2",    // COM1 + 3
            "mov al, 0x03",
            "out dx, al",   // 8 bits, no parity, one stop bit

            "dec dx",       // COM1 + 2
            "mov al, 0xc7",
            "out dx, al",   // Enable FIFO, clear them, with 14-byte threshold

            "add dx, 2",    // COM1 + 4
            "mov al, 0x0b",
            "out dx, al",   // IRQs enabled, RTS/DSR set

            com1_base = const COM1,
            out("dx") _,
            out("al") _
        );
    }
}

pub fn serial_putchar(byte: u8) {
    unsafe {
        asm!(
            "mov dx, {com1_base}",
            "add dx, 5", // COM1 + 5
            "2:",
            "in al, dx",
            "test al, 0x20",
            "jz 2b", // Wait until transmitter is ready

            "mov dx, {com1_base}", // COM1
            "mov al, {byte}",
            "out dx, al", // Write byte

            com1_base = const COM1,
            byte = in(reg_byte) byte,
            out("dx") _,
            out("al") _
        );
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
    let rsp: usize;
    unsafe { asm!("mov {}, rsp", out(reg) rsp); }
    return rsp as *const u8;
}

// ALL STACK VARIABLES ARE VOID BEYOND THIS POINT.
#[inline(always)]
pub unsafe fn move_stack(ptr: &OwnedPtr) {
    unsafe {
        ptr.ptr::<u8>().write_bytes(0, ptr.size());
        asm!("mov rsp, {}", in(reg) ptr.addr() + ptr.size());
    }
}
