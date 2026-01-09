use core::arch::{asm, global_asm};

unsafe extern "C" {
    unsafe fn exc_vts();
}

macro_rules! exc_stub {
    ($n:tt) => {
        concat!(
            "sub sp, sp, #816\n",
            "stp x0, x1, [sp, #0]\n",
            "mov x0, #", stringify!($n), "\n",
            "b exc_entry\n",
            ".align 7\n"
        )
    };
}

global_asm!(
    ".align 11",
    ".global exc_vts",
    "exc_vts:",
        exc_stub!(0),  exc_stub!(1),  exc_stub!(2),  exc_stub!(3),
        exc_stub!(4),  exc_stub!(5),  exc_stub!(6),  exc_stub!(7),
        exc_stub!(8),  exc_stub!(9),  exc_stub!(10), exc_stub!(11),
        exc_stub!(12), exc_stub!(13), exc_stub!(14), exc_stub!(15),

    "exc_entry:",
        "stp x2, x3, [sp, #16]",
        "stp x4, x5, [sp, #32]",
        "stp x6, x7, [sp, #48]",
        "stp x8, x9, [sp, #64]",
        "stp x10, x11, [sp, #80]",
        "stp x12, x13, [sp, #96]",
        "stp x14, x15, [sp, #112]",
        "stp x16, x17, [sp, #128]",
        "stp x18, x19, [sp, #144]",
        "stp x20, x21, [sp, #160]",
        "stp x22, x23, [sp, #176]",
        "stp x24, x25, [sp, #192]",
        "stp x26, x27, [sp, #208]",
        "stp x28, x29, [sp, #224]",
        "str x30, [sp, #240]",
        "str xzr, [sp, #248]",

        "mrs x2, elr_el1",
        "mrs x3, spsr_el1",
        "stp x2, x3, [sp, #256]",
        "mrs x2, esr_el1",
        "mrs x3, far_el1",
        "stp x2, x3, [sp, #272]",

        "stp q0, q1, [sp, #288]",
        "stp q2, q3, [sp, #320]",
        "stp q4, q5, [sp, #352]",
        "stp q6, q7, [sp, #384]",
        "stp q8, q9, [sp, #416]",
        "stp q10, q11, [sp, #448]",
        "stp q12, q13, [sp, #480]",
        "stp q14, q15, [sp, #512]",
        "stp q16, q17, [sp, #544]",
        "stp q18, q19, [sp, #576]",
        "stp q20, q21, [sp, #608]",
        "stp q22, q23, [sp, #640]",
        "stp q24, q25, [sp, #672]",
        "stp q26, q27, [sp, #704]",
        "stp q28, q29, [sp, #736]",
        "stp q30, q31, [sp, #768]",

        "mrs x2, fpcr",
        "str x2, [sp, #800]",
        "mrs x2, fpsr",
        "str x2, [sp, #808]",

        "mov x1, sp",
        "bl exc_handler",

        "ldr x2, [sp, #800]",
        "msr fpcr, x2",
        "ldr x2, [sp, #808]",
        "msr fpsr, x2",

        "ldp q0, q1, [sp, #288]",
        "ldp q2, q3, [sp, #320]",
        "ldp q4, q5, [sp, #352]",
        "ldp q6, q7, [sp, #384]",
        "ldp q8, q9, [sp, #416]",
        "ldp q10, q11, [sp, #448]",
        "ldp q12, q13, [sp, #480]",
        "ldp q14, q15, [sp, #512]",
        "ldp q16, q17, [sp, #544]",
        "ldp q18, q19, [sp, #576]",
        "ldp q20, q21, [sp, #608]",
        "ldp q22, q23, [sp, #640]",
        "ldp q24, q25, [sp, #672]",
        "ldp q26, q27, [sp, #704]",
        "ldp q28, q29, [sp, #736]",
        "ldp q30, q31, [sp, #768]",

        "ldp x2, x3, [sp, #256]",
        "msr elr_el1, x2",
        "msr spsr_el1, x3",

        "ldp x2, x3, [sp, #16]",
        "ldp x4, x5, [sp, #32]",
        "ldp x6, x7, [sp, #48]",
        "ldp x8, x9, [sp, #64]",
        "ldp x10, x11, [sp, #80]",
        "ldp x12, x13, [sp, #96]",
        "ldp x14, x15, [sp, #112]",
        "ldp x16, x17, [sp, #128]",
        "ldp x18, x19, [sp, #144]",
        "ldp x20, x21, [sp, #160]",
        "ldp x22, x23, [sp, #176]",
        "ldp x24, x25, [sp, #192]",
        "ldp x26, x27, [sp, #208]",
        "ldp x28, x29, [sp, #224]",
        "ldr x30, [sp, #240]",

        "ldp x0, x1, [sp, #0]",
        "add sp, sp, #816",

        "eret"
);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InterFrame {
    pub x: [u64; 32],  // x regs
    pub elr: u64,      // ret addr
    pub spsr: u64,     // saved pstate
    pub esr: u64,      // exc syndrome
    pub far: u64,      // fault addr
    pub q: [u128; 32], // q regs
    pub fpcr: u64,     // fp ctrl reg
    pub fpsr: u64      // fp status reg
}

#[unsafe(no_mangle)]
extern "C" fn exc_handler(exc_type: u64, frame: &mut InterFrame) {
    match exc_type {
        // 0  => { /* sync el1t */ }
        // 1  => { /* irq  el1t */ }
        // 2  => { /* fiq  el1t */ }
        // 3  => { /* serr el1t */ }
        // 4  => { /* sync el1h */ }
        // 5  => { /* irq  el1h */ }
        // 6  => { /* fiq  el1h */ }
        // 7  => { /* serr el1h */ }
        8  | 12 => { /* sync el0 */
            if (frame.esr >> 26) & 0x3f == 0x15 { // supervisor call
                frame.x[0] = crate::kreq::kernel_requestee(
                    frame.x[0] as *const u8,
                    frame.x[1] as usize, frame.x[2] as usize, frame.x[3] as usize,
                    frame.x[4] as usize, frame.x[5] as usize, frame.x[6] as usize
                ) as u64;
            }
        }
        // 9  | 13 => { /* irq  el0  */ }
        // 10 | 14 => { /* fiq  el0  */ }
        // 11 | 15 => { /* serr el0  */ }
        ..16 => {
            crate::printlnk!("Exception type: {}", exc_type);
            crate::printlnk!("Exception frame: {:#x?}", frame);

            panic!("Unhandled exception");
        }
        _ => unreachable!()
    }
}

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
            asm!("msr daifclr, #0b1111", options(nomem, nostack));
        } else {
            asm!("msr daifset, #0b1111", options(nomem, nostack));
        }
    }
}

pub fn init() {
    unsafe {
        asm!(
            "msr tpidr_el1, {}",
            "msr vbar_el1, {}",
            in(reg) &crate::ram::stack_top(),
            in(reg) exc_vts,
            options(nostack, preserves_flags)
        );
    }
}
