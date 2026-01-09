use crate::{arch::inter::InterFrame, ram::stack_top};

use core::arch::asm;

impl InterFrame {
    pub const fn new() -> Self {
        return Self {
            xmm: [0u128; 16],
            mxcsr: 0x1f80,
            _0: 0,
            r15: 0, r14: 0, r13: 0, r12: 0,
            r11: 0, r10: 0, r9: 0, r8: 0,
            rbp: 0, rdi: 0, rsi: 0, rdx: 0,
            rcx: 0, rbx: 0, rax: 0,
            vec: 0, err: 0,
            rip: 0, cs: 0x23, rflags: 0x202, rsp: 0, ss: 0x1b
        };
    }

    pub const fn pc(&self) -> usize {
        return self.rip as usize;
    }

    pub const fn sp(&self) -> usize {
        return self.rsp as usize;
    }

    pub const fn arg(&self, arg_i: usize) -> usize {
        match arg_i {
            0 => self.rdi as usize,
            1 => self.rsi as usize,
            2 => self.rdx as usize,
            3 => self.rcx as usize,
            4 => self.r8 as usize,
            5 => self.r9 as usize,
            _ => 0
        }
    }

    pub const fn set_pc(&mut self, pc: usize) {
        self.rip = pc as u64;
    }

    pub const fn set_sp(&mut self, sp: usize) {
        self.rsp = sp as u64;
    }

    pub const fn set_arg(&mut self, arg_i: usize, arg: usize) {
        match arg_i {
            0 => self.rdi = arg as u64,
            1 => self.rsi = arg as u64,
            2 => self.rdx = arg as u64,
            3 => self.rcx = arg as u64,
            4 => self.r8 = arg as u64,
            5 => self.r9 = arg as u64,
            _ => {}
        }
    }
}

#[inline(always)]
pub unsafe fn rstr_ctxt(ctxt: &InterFrame) -> ! {
    unsafe {
        asm!(
            "mov r14, {ksp}",
            "mov r15, {ctxt}",

            "ldmxcsr [r15 + 256]",

            "movaps xmm0, [r15 + 0]",
            "movaps xmm1, [r15 + 16]",
            "movaps xmm2, [r15 + 32]",
            "movaps xmm3, [r15 + 48]",
            "movaps xmm4, [r15 + 64]",
            "movaps xmm5, [r15 + 80]",
            "movaps xmm6, [r15 + 96]",
            "movaps xmm7, [r15 + 112]",
            "movaps xmm8, [r15 + 128]",
            "movaps xmm9, [r15 + 144]",
            "movaps xmm10, [r15 + 160]",
            "movaps xmm11, [r15 + 176]",
            "movaps xmm12, [r15 + 192]",
            "movaps xmm13, [r15 + 208]",
            "movaps xmm14, [r15 + 224]",
            "movaps xmm15, [r15 + 240]",

            "mov rax, [r15 + 384]",
            "mov rbx, [r15 + 376]",
            "mov rcx, [r15 + 368]",
            "mov rdx, [r15 + 360]",
            "mov rsi, [r15 + 352]",
            "mov rdi, [r15 + 344]",
            "mov rbp, [r15 + 336]",
            "mov r8, [r15 + 328]",
            "mov r9, [r15 + 320]",
            "mov r10, [r15 + 312]",
            "mov r11, [r15 + 304]",
            "mov r12, [r15 + 296]",
            "mov r13, [r15 + 288]",

            "mov rsp, r14",

            "push qword ptr [r15 + 440]",
            "push qword ptr [r15 + 432]",
            "push qword ptr [r15 + 424]",
            "push qword ptr [r15 + 416]",
            "push qword ptr [r15 + 408]",

            "mov r14, [r15 + 280]",
            "mov r15, [r15 + 272]",

            "iretq",
            ctxt = in(reg) ctxt,
            ksp = in(reg) stack_top(),
            options(noreturn)
        );
    }
}
