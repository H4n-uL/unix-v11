pub mod exc;
pub mod intc;
pub mod proc;
pub mod rvm;

use core::{arch::asm, fmt::{Result, Write}};

pub fn wfi() {
    exc::set(true);
    unsafe { asm!("hlt"); }
}

pub fn halt() {
    exc::set(false);
    unsafe { asm!("hlt"); }
}

pub const R_REL: usize    = 8; // R_RELATIVE
pub const R_SYM: &[usize] = &[
    1, // R_64:        S + A
    6, // R_GLOB_DAT:  S
    7  // R_JUMP_SLOT: S
];

const COM1: u16 = 0x3f8;

#[inline(always)]
pub fn phys_id() -> usize {
    let apic_id: u32;
    unsafe {
        asm!(
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",
            "mov eax, 1",
            "cpuid",
            "mov {0:e}, ebx",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",
            out(reg) apic_id
        );
    }
    return (apic_id >> 24) as usize;
}

pub fn init_serial() {
    unsafe {
        asm!(
            "mov dx, {com1_base}",
            "inc dx",       // COM1 + 1
            "mov al, 0x00",
            "out dx, al",   // Disable all interrupts

            "add dx, 2",    // COM1 + 3
            "mov al, 0x80", // Enable DLAB (set baud rate divisor)
            "out dx, al",

            "sub dx, 3",    // COM1 + 0
            "mov al, 0x01", // Set divisor to 1 (lo byte) 115200 baud
            "out dx, al",

            "inc dx",       // COM1 + 1
            "mov al, 0x00", //                  (hi byte)
            "out dx, al",

            "add dx, 2",    // COM1 + 3
            "mov al, 0x03", // 8 bits, no parity, one stop bit
            "out dx, al",

            "dec dx",       // COM1 + 2
            "mov al, 0xc7", // Enable FIFO, clear them, with 14-byte threshold
            "out dx, al",

            "add dx, 2",    // COM1 + 4
            "mov al, 0x0b", // IRQs enabled, RTS/DSR set
            "out dx, al",

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

#[inline(always)]
pub unsafe fn move_stack(addr: usize) {
    unsafe {
        asm!("mov rsp, {}", in(reg) addr);
    }
}
