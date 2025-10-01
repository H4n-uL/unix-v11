use crate::ram::glacier::{Glacier, MMUCfg, PageSize};

#[allow(dead_code)]
pub mod flags {
    pub const VALID: usize = 0b1;
    pub const NEXT: usize  = 0b11;

    pub const K_ROO: usize = 0b1 | 1 << 63;
    pub const K_RWO: usize = 0b11 | 1 << 63;
    pub const K_ROX: usize = 0b1;
    pub const K_RWX: usize = 0b11;

    pub const D_RO: usize  = 0b10001 | 1 << 63;
    pub const D_RW: usize  = 0b10011 | 1 << 63;

    pub const U_ROO: usize = 0b101 | 1 << 63;
    pub const U_RWO: usize = 0b111 | 1 << 63;
    pub const U_ROX: usize = 0b101;
    pub const U_RWX: usize = 0b111;
}

impl MMUCfg {
    pub fn detect() -> Self {
        return Self {
            page_size: PageSize::Size4kiB,
            va_bits: 48,
            pa_bits: 52
        };
    }
}

impl Glacier {
    pub fn identity_map(&self) {
        unsafe {
            core::arch::asm!(
                "mov cr3, {pml4}",

                "mov rax, cr0",
                "mov rbx, 0x80000000",
                "or rax, rbx",
                "mov cr0, rax",

                "mov rax, cr4",
                "or eax, 0x00000030", // PAE / PSE
                "mov cr4, rax",

                "mov ecx, 0xc0000080",
                "rdmsr",
                "or eax, 0x00000900", // NXE / LME
                "wrmsr",

                pml4 = in(reg) self.root_table()
            );
        }
    }
}
