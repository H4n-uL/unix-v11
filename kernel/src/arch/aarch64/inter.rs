use core::arch::{asm, global_asm};

pub fn get() -> bool {
    let daif: u64;
    unsafe {
        asm!("mrs {}, daif", out(reg) daif, options(nomem, nostack, preserves_flags));
    }
    return (daif & 0b1111) != 0;
}

pub fn set(enabled: bool) {
    unsafe {
        if enabled {
            asm!("msr daifset, #0b1111", options(nomem, nostack, preserves_flags));
        } else {
            asm!("msr daifclr, #0b1111", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe extern "C" {
    unsafe fn exc_vts();
}

global_asm!(
    ".align 11",
    ".global exc_vts",
    "exc_vts:",
        "str x0, [sp, #-16]!",
        "mov x0, #0",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #1",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #2",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #3",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #4",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #5",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #6",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #7",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #8",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #9",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #10",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #11",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #12",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #13",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #14",
        "b exc_entry",
        ".align 7",

        "str x0, [sp, #-16]!",
        "mov x0, #15",
        "b exc_entry",
        ".align 7",

    "exc_entry:",
        "sub sp, sp, #272",
        "stp x1, x2, [sp, #0]",
        "stp x3, x4, [sp, #16]",
        "stp x5, x6, [sp, #32]",
        "stp x7, x8, [sp, #48]",
        "stp x9, x10, [sp, #64]",
        "stp x11, x12, [sp, #80]",
        "stp x13, x14, [sp, #96]",
        "stp x15, x16, [sp, #112]",
        "stp x17, x18, [sp, #128]",
        "stp x19, x20, [sp, #144]",
        "stp x21, x22, [sp, #160]",
        "stp x23, x24, [sp, #176]",
        "stp x25, x26, [sp, #192]",
        "stp x27, x28, [sp, #208]",
        "stp x29, x30, [sp, #224]",

        "mrs x1, elr_el1",
        "mrs x2, spsr_el1",
        "stp x1, x2, [sp, #240]",

        "mrs x1, esr_el1",
        "mrs x2, far_el1",
        "stp x1, x2, [sp, #256]",

        "mov x1, sp",
        "bl exc_handler",

        "ldp x1, x2, [sp, #240]",
        "msr elr_el1, x1",
        "msr spsr_el1, x2",

        "ldp x1, x2, [sp, #0]",
        "ldp x3, x4, [sp, #16]",
        "ldp x5, x6, [sp, #32]",
        "ldp x7, x8, [sp, #48]",
        "ldp x9, x10, [sp, #64]",
        "ldp x11, x12, [sp, #80]",
        "ldp x13, x14, [sp, #96]",
        "ldp x15, x16, [sp, #112]",
        "ldp x17, x18, [sp, #128]",
        "ldp x19, x20, [sp, #144]",
        "ldp x21, x22, [sp, #160]",
        "ldp x23, x24, [sp, #176]",
        "ldp x25, x26, [sp, #192]",
        "ldp x27, x28, [sp, #208]",
        "ldp x29, x30, [sp, #224]",
        "add sp, sp, #272",
        "ldr x0, [sp], #16",

        "eret",
);

#[repr(C)]
#[derive(Debug)]
pub struct ExceptionFrame {
    pub x1_x28: [u64; 28],  // x1-x28
    pub x29: u64,           // frame pointer
    pub x30: u64,           // link register
    pub elr: u64,           // return address
    pub spsr: u64,          // saved program status
    pub esr: u64,           // exception syndrome
    pub far: u64,           // fault address
    pub x0: u64,            // x0
    pub _0: u64
}

#[unsafe(no_mangle)]
extern "C" fn exc_handler(exc_type: u64, frame: &mut ExceptionFrame) {
    match exc_type {
        0  => { /* sync el1t */ }
        1  => { /* irq  el1t */ }
        2  => { /* fiq  el1t */ }
        3  => { /* serr el1t */ }
        4  => { /* sync el1h */ }
        5  => { /* irq  el1h */ }
        6  => { /* fiq  el1h */ }
        7  => { /* serr el1h */ }
        8  => { /* sync el0  */ }
        9  => { /* irq  el0  */ }
        10 => { /* fiq  el0  */ }
        11 => { /* serr el0  */ }
        12 => { /* sync el0  */ }
        13 => { /* irq  el0  */ }
        14 => { /* fiq  el0  */ }
        15 => { /* serr el0  */ }
        _ => unreachable!(),
    }
    crate::printlnk!("Exception type: {}", exc_type);
    crate::printlnk!("Exception frame: {:#?}", frame);
}

pub fn init() {
    unsafe {
        asm!(
            "msr vbar_el1, {}",
            in(reg) exc_vts,
            options(nostack, preserves_flags)
        );
    }
}
