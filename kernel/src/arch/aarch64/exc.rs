use crate::{
    arch::intc,
    kreq::kernel_requestee,
    printlnk, ram::stack_top
};

use core::arch::{asm, global_asm};

unsafe extern "C" {
    unsafe fn exc_vts();
}

// EL1t stub: kernel exceptions (from spsel == 0)
macro_rules! exc_stub_el1t {
    ($n:tt) => {
        concat!(
            "sub sp, sp, #816\n",      // sp -= size_of::<ExcFrame>()
            "stp x0, x1, [sp, #0]\n",  // frame.x[0..2] = x0, x1
            "stp x2, x3, [sp, #16]\n", // frame.x[2..4] = x2, x3
            "mrs x0, sp_el0\n",        // x0 = sp_el0 (user sp)
            "str x0, [sp, #248]\n",    // frame.x[31] = sp_el0 (user sp)
            "mov x1, sp\n",
            "mov x0, #", stringify!($n), "\n",
            "b exc_entry\n",
            ".align 7\n"
        )
    };
}

// EL1h stub: double fault (from spsel == 1, panic)
macro_rules! exc_stub_el1h {
    ($n:tt) => {
        concat!(
            "b double_fault_panic\n",
            ".align 7\n"
        )
    };
}

// EL0 stub: user exceptions
macro_rules! exc_stub_el0 {
    ($n:tt) => {
        concat!(
            "stp x0, x1, [sp, #-32]!\n",
            "stp x2, x3, [sp, #16]\n",
            "mrs x0, sp_el0\n",
            "mrs x1, tpidr_el1\n",
            "sub x1, x1, #816\n",
            "str x0, [x1, #248]\n",
            "ldp x2, x3, [sp, #0]\n",
            "stp x2, x3, [x1, #0]\n",
            "ldp x2, x3, [sp, #16]\n",
            "stp x2, x3, [x1, #16]\n",
            "add sp, sp, #32\n",
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
        // EL1t (0-3):  spsel=0, kernel exceptions
        exc_stub_el1t!(0),  exc_stub_el1t!(1),
        exc_stub_el1t!(2),  exc_stub_el1t!(3),
        // EL1h (4-7):  spsel=1, double fault (panic)
        exc_stub_el1h!(4),  exc_stub_el1h!(5),
        exc_stub_el1h!(6),  exc_stub_el1h!(7),
        // EL0 (8-11):  AArch64 user exceptions
        exc_stub_el0!(8),   exc_stub_el0!(9),
        exc_stub_el0!(10),  exc_stub_el0!(11),
        // EL0 (12-15): AArch32 user exceptions (will never be supported)
        exc_stub_el0!(12),  exc_stub_el0!(13),
        exc_stub_el0!(14),  exc_stub_el0!(15),

    // x0 = exc_type, x1 = ExcFrame base
    // (x0-x3, x31 saved by stub)
    "exc_entry:",
        "stp  x4,  x5, [x1, #32]",
        "stp  x6,  x7, [x1, #48]",
        "stp  x8,  x9, [x1, #64]",
        "stp x10, x11, [x1, #80]",
        "stp x12, x13, [x1, #96]",
        "stp x14, x15, [x1, #112]",
        "stp x16, x17, [x1, #128]",
        "stp x18, x19, [x1, #144]",
        "stp x20, x21, [x1, #160]",
        "stp x22, x23, [x1, #176]",
        "stp x24, x25, [x1, #192]",
        "stp x26, x27, [x1, #208]",
        "stp x28, x29, [x1, #224]",
        "str x30, [x1, #240]",

        "mrs x2, elr_el1",
        "mrs x3, spsr_el1",
        "stp x2, x3, [x1, #256]",
        "mrs x2, esr_el1",
        "mrs x3, far_el1",
        "stp x2, x3, [x1, #272]",

        "stp  q0,  q1, [x1, #288]",
        "stp  q2,  q3, [x1, #320]",
        "stp  q4,  q5, [x1, #352]",
        "stp  q6,  q7, [x1, #384]",
        "stp  q8,  q9, [x1, #416]",
        "stp q10, q11, [x1, #448]",
        "stp q12, q13, [x1, #480]",
        "stp q14, q15, [x1, #512]",
        "stp q16, q17, [x1, #544]",
        "stp q18, q19, [x1, #576]",
        "stp q20, q21, [x1, #608]",
        "stp q22, q23, [x1, #640]",
        "stp q24, q25, [x1, #672]",
        "stp q26, q27, [x1, #704]",
        "stp q28, q29, [x1, #736]",
        "stp q30, q31, [x1, #768]",

        "mrs x2, fpcr",
        "str x2, [x1, #800]",
        "mrs x2, fpsr",
        "str x2, [x1, #808]",

        "msr sp_el0, x1",
        "msr spsel, #0",
        "mov x1, sp",
        "bl exc_handler",

        "ldp x2, x3, [sp, #256]",
        "msr elr_el1, x2",
        "msr spsr_el1, x3",

        "ldr x2, [sp, #800]",
        "msr fpcr, x2",
        "ldr x2, [sp, #808]",
        "msr fpsr, x2",

        "ldp  q0,  q1, [sp, #288]",
        "ldp  q2,  q3, [sp, #320]",
        "ldp  q4,  q5, [sp, #352]",
        "ldp  q6,  q7, [sp, #384]",
        "ldp  q8,  q9, [sp, #416]",
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

        "ldr x2, [sp, #264]",
        "tst x2, #0b1100", // set EL0h flag (for b.ne)

        "mov x9, sp",
        "msr spsel, #1",
        "ldr x10, [x9, #248]",
        "msr sp_el0, x10",

        "ldp  x0,  x1, [x9, #0]",
        "ldp  x2,  x3, [x9, #16]",
        "ldp  x4,  x5, [x9, #32]",
        "ldp  x6,  x7, [x9, #48]",
        "ldr  x8,      [x9, #64]",
        "ldp x10, x11, [x9, #80]",
        "ldp x12, x13, [x9, #96]",
        "ldp x14, x15, [x9, #112]",
        "ldp x16, x17, [x9, #128]",
        "ldp x18, x19, [x9, #144]",
        "ldp x20, x21, [x9, #160]",
        "ldp x22, x23, [x9, #176]",
        "ldp x24, x25, [x9, #192]",
        "ldp x26, x27, [x9, #208]",
        "ldp x28, x29, [x9, #224]",
        "ldr x30,      [x9, #240]",
        "ldr  x9,      [x9, #72]",

        "b.ne 2f", // branch here (tst x2, #0b1100)
        "eret",
    "2: add sp, sp, #816",
        "eret",

    "double_fault_panic:",
        "msr daifset, #0b1111",
        "1: wfe",
        "b 1b"
);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ExcFrame {
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
extern "C" fn exc_handler(exc_type: u64, frame: *mut ExcFrame) {
    macro_rules! ref_frame {
        () => { unsafe { *frame } };
    }

    match exc_type {
        0 => { /* sync el1t */
            printlnk!("Kernel sync exception (EL1t)");
            printlnk!("Exception frame: {:#x?}", frame);
            panic!("Unhandled kernel exception");
        }
        1 => { /* irq el1t */
            let intid = intc::ack();
            match intid {
                27 => { // timer
                    printlnk!("Timer IRQ");
                    intc::timer_set_ms(1000);
                }
                _ => {
                    printlnk!("Unhandled IRQ: {}", intid);
                }
            }
            intc::eoi(intid);
        }
        // 2  => { /* fiq  el1t */ }
        // 3  => { /* serr el1t */ }
        4..8 => unreachable!(),
        8  | 12 => { /* sync el0 */
            if (ref_frame!().esr >> 26) & 0x3f == 0x15 { // supervisor call
                ref_frame!().x[0] = kernel_requestee(
                    ref_frame!().x[0] as *const u8,
                    ref_frame!().x[1] as usize, ref_frame!().x[2] as usize, ref_frame!().x[3] as usize,
                    ref_frame!().x[4] as usize, ref_frame!().x[5] as usize, ref_frame!().x[6] as usize
                ) as u64;
            } else {
                printlnk!("Exception type: {}", exc_type);
                printlnk!("Exception frame: {:#x?}", ref_frame!());
                panic!("Unhandled exception");
            }
        }
        9  | 13 => { /* irq el0 */
            let intid = intc::ack();
            match intid {
                27 => { // timer
                    printlnk!("Timer IRQ");
                }
                _ => {
                    printlnk!("Unhandled IRQ: {}", intid);
                }
            }
            intc::eoi(intid);
        }
        // 10 | 14 => { /* fiq  el0  */ }
        // 11 | 15 => { /* serr el0  */ }
        ..16 => {
            printlnk!("Exception type: {}", exc_type);
            printlnk!("Exception frame: {:#x?}", ref_frame!());

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
    return (daif & (0b1111 << 6)) == 0;
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
            "msr vbar_el1, {vbar}",
            "msr tpidr_el1, xzr",

            "mov {tmp}, sp",
            "msr sp_el0, {tmp}",
            "mov sp, {tramp}",
            "msr spsel, #0",

            vbar = in(reg) exc_vts,
            tramp = in(reg) stack_top(),
            tmp = out(reg) _
        );
    }
}

pub fn set_kstk(kstk_top: usize) {
    unsafe {
        asm!("msr tpidr_el1, {}", in(reg) kstk_top, options(nomem, nostack));
    }
}
