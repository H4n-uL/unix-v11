use crate::arch::inter::InterFrame;

use core::arch::asm;

impl InterFrame {
    pub const fn new() -> Self {
        return Self {
            x: [0u64; 32],
            elr: 0,
            spsr: 0,
            esr: 0,
            far: 0,
            q: [0u128; 32],
            fpcr: 0,
            fpsr: 0
        };
    }

    pub const fn pc(&self) -> usize {
        return self.elr as usize;
    }

    pub const fn sp(&self) -> usize {
        return self.x[31] as usize;
    }

    pub const fn arg(&self, arg_i: usize) -> usize {
        return self.x[arg_i] as usize;
    }

    pub const fn set_pc(&mut self, pc: usize) {
        self.elr = pc as u64;
    }

    pub const fn set_sp(&mut self, sp: usize) {
        self.x[31] = sp as u64;
    }

    pub const fn set_arg(&mut self, arg_i: usize, arg: usize) {
        self.x[arg_i] = arg as u64;
    }
}

#[inline(always)]
pub unsafe fn rstr_ctxt(ctxt: &InterFrame) -> ! {
    unsafe {
        asm!(
            "mov x9, {ctxt}",
            "mov x8, {ksp}",

            "ldr x10, [x9, #256]",
            "msr elr_el1, x10",
            "ldr x10, [x9, #264]",
            "msr spsr_el1, x10",
            "ldr x10, [x9, #248]",
            "msr sp_el0, x10",
            "ldr x10, [x9, #800]",
            "msr fpcr, x10",
            "ldr x10, [x9, #808]",
            "msr fpsr, x10",

            "ldp q0, q1, [x9, #288]",
            "ldp q2, q3, [x9, #320]",
            "ldp q4, q5, [x9, #352]",
            "ldp q6, q7, [x9, #384]",
            "ldp q8, q9, [x9, #416]",
            "ldp q10, q11, [x9, #448]",
            "ldp q12, q13, [x9, #480]",
            "ldp q14, q15, [x9, #512]",
            "ldp q16, q17, [x9, #544]",
            "ldp q18, q19, [x9, #576]",
            "ldp q20, q21, [x9, #608]",
            "ldp q22, q23, [x9, #640]",
            "ldp q24, q25, [x9, #672]",
            "ldp q26, q27, [x9, #704]",
            "ldp q28, q29, [x9, #736]",
            "ldp q30, q31, [x9, #768]",

            "ldp x0, x1, [x9, #0]",
            "ldp x2, x3, [x9, #16]",
            "ldp x4, x5, [x9, #32]",
            "ldp x6, x7, [x9, #48]",
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
            "ldr x30, [x9, #240]",

            "mov sp, x8",
            "ldr x8, [x9, #64]",
            "ldr x9, [x9, #72]",
            "eret",
            ctxt = in(reg) ctxt,
            ksp = in(reg) crate::ram::stack_top(),
            options(noreturn)
        );
    }
}
