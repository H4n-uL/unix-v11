use crate::{
    ram::glacier::{GlacierData, MMUCfg, PageSize},
    sysinfo::ramtype
};

#[allow(dead_code)]
pub mod flags {
    // Descriptor type bits [1:0]
    pub const VALID: usize   = 1 << 0;  // Present bit
    const WRITABLE: usize    = 1 << 1;  // Writable bit

    // Access permissions
    const AP_EL1: usize      = 0;       // EL1 only
    const AP_EL0: usize      = 1 << 2;  // Both EL1 and EL0

    // Attributes
    const ATTR_NORMAL: usize = 0 << 4;  // Normal memory (cacheable)
    const ATTR_DEVICE: usize = 1 << 4;  // Device memory (PCD bit)

    // Other flags
    const LARGE: usize       = 1 << 7;  // Large Page
    const UXN: usize         = 1 << 63; // No execute (NX bit)
    const PXN: usize         = 1 << 63; // Same as UXN for x86

    pub const NEXT_TABLE: usize    = VALID | WRITABLE | ATTR_NORMAL | AP_EL1;
    pub const PAGE_DEFAULT: usize  = VALID | WRITABLE | ATTR_NORMAL | AP_EL1;
    pub const PAGE_NOEXEC: usize   = VALID | WRITABLE | ATTR_NORMAL | AP_EL1 | UXN;
    pub const PAGE_DEVICE: usize   = VALID | WRITABLE | ATTR_DEVICE | AP_EL1 | UXN;
    pub const LARGE_DEFAULT: usize = VALID | WRITABLE | ATTR_NORMAL | AP_EL1 | LARGE;
    pub const LARGE_NOEXEC: usize  = VALID | WRITABLE | ATTR_NORMAL | AP_EL1 | UXN | LARGE;
    pub const LARGE_DEVICE: usize  = VALID | WRITABLE | ATTR_DEVICE | AP_EL1 | UXN | LARGE;
}

pub fn flags_for_type(ty: u32) -> usize {
    use flags::*;
    match ty {
        ramtype::CONVENTIONAL => PAGE_DEFAULT,
        ramtype::BOOT_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::RUNTIME_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::KERNEL => PAGE_DEFAULT,
        ramtype::KERNEL_DATA => PAGE_NOEXEC,
        ramtype::KERNEL_PAGE_TABLE => PAGE_NOEXEC,
        ramtype::MMIO => PAGE_DEVICE,
        _ => PAGE_NOEXEC
    }
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

pub fn identity_map(glacier: &GlacierData) {
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

            pml4 = in(reg) glacier.root_table()
        );
    }
}