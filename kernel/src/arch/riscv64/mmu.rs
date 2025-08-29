use crate::{
    ram::glacier::{GlacierData, MMUCfg, PageSize},
    sysinfo::ramtype
};

#[allow(dead_code)]
pub mod flags {
    // Descriptor type
    pub const VALID: usize   = 1 << 0; // V bit - Present
    const READ: usize        = 1 << 1; // R bit
    const WRITE: usize       = 1 << 2; // W bit
    const EXEC: usize        = 1 << 3; // X bit
    const USER: usize        = 1 << 4; // U bit - User accessible
    const GLOBAL: usize      = 1 << 5; // G bit
    const AF: usize          = 1 << 6; // A bit - Accessed flag
    const DIRTY: usize       = 1 << 7; // D bit

    // Attributes
    const ATTR_NORMAL: usize = 0 << 8; // Normal (cacheable)
    const ATTR_DEVICE: usize = 1 << 8; // Device (uncached)

    pub const NEXT_TABLE: usize    = VALID;
    pub const PAGE_DEFAULT: usize  = VALID | READ | WRITE | EXEC | AF | ATTR_NORMAL;
    pub const PAGE_NOEXEC: usize   = VALID | READ | WRITE | AF | ATTR_NORMAL;
    pub const PAGE_DEVICE: usize   = VALID | READ | WRITE | AF | ATTR_DEVICE;
    pub const LARGE_DEFAULT: usize = PAGE_DEFAULT;
    pub const LARGE_NOEXEC: usize  = PAGE_NOEXEC;
    pub const LARGE_DEVICE: usize  = PAGE_DEVICE;
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
        Self {
            page_size: PageSize::Size4kiB,
            va_bits: 48,
            pa_bits: 56
        }
    }
}

pub fn identity_map(glacier: &GlacierData) {
    unsafe {
        core::arch::asm!(
            // Mode: Sv48 (9), ASID: 0, PPN: root_table >> 12
            "li t0, 9",                  // Sv48 mode
            "slli t0, t0, 60",           // Shift mode to bits [63:60]
            "srli t1, {root_table}, 12", // Get PPN from root table address
            "or t0, t0, t1",             // Combine mode and PPN

            "sfence.vma",
            "csrw satp, t0",
            "sfence.vma",

            "li t0, 0x00020000", // SUM bit
            "csrs sstatus, t0",  // Set in sstatus
            root_table = in(reg) glacier.root_table()
        );
    }
}