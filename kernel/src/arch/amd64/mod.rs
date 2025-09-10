/* pub mod exceptions; */ pub mod mmu;

use crate::{ram::physalloc::OwnedPtr, SYS_INFO};

pub fn set_interrupts(enabled: bool) {
    unsafe {
        if enabled {
            core::arch::asm!("sti");
        } else {
            core::arch::asm!("cli");
        }
    }
}

pub fn halt() {
    set_interrupts(false);
    unsafe { core::arch::asm!("hlt"); }
}

pub const R_RELATIVE: u64 = 8;
const COM1: u16 = 0x3f8;

pub fn init_serial() {
    unsafe {
        core::arch::asm!(
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
        core::arch::asm!(
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

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() { serial_putchar(byte); }
        Ok(())
    }
}

#[inline(always)]
pub fn stack_ptr() -> *const u8 {
    let rsp: usize;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp); }
    return rsp as *const u8;
}

pub unsafe fn move_stack(ptr: &OwnedPtr) {
    let stack_ptr = stack_ptr();
    let old_stack_base = SYS_INFO.lock().stack_base;
    let stack_size = old_stack_base.saturating_sub(stack_ptr as usize);

    let new_stack_base = ptr.addr() + ptr.size();
    let new_stack_bottom = new_stack_base.saturating_sub(stack_size) as *mut u8;

    unsafe {
        core::ptr::copy(stack_ptr, new_stack_bottom, stack_size);
        core::arch::asm!("mov rsp, {}", in(reg) new_stack_bottom);
    }

    SYS_INFO.set_new_stack_base(new_stack_base);
}