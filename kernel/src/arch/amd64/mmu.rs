use crate::{
    ram::{glacier::{MMUCfg, PageSize, GLACIER}, PAGE_4KIB},
    sysinfo::ramtype,
    SYS_INFO
};

#[allow(dead_code)]
pub mod flags {
    pub const VALID: usize       = 1 << 0;  // Present bit
    pub const TABLE_DESC: usize  = 0x03;    // Present + Writable for non-leaf
    pub const PAGE_DESC: usize   = 0x03;    // Present + Writable for leaf
    pub const AF: usize          = 1 << 5;  // Accessed flag

    pub const ATTR_NORMAL: usize = 0 << 4;  // Normal memory (cacheable)
    pub const ATTR_DEVICE: usize = 1 << 4;  // Device memory (PCD bit)

    pub const AP_EL1: usize      = 0;       // EL1 only
    pub const AP_EL0: usize      = 1 << 2;  // Both EL1 and EL0

    pub const UXN: usize         = 1 << 63; // No execute (NX bit)
    pub const PXN: usize         = 1 << 63; // Same as UXN for x86

    pub const PAGE_DEFAULT: usize = PAGE_DESC | AF | ATTR_NORMAL | AP_EL1;
    pub const PAGE_NOEXEC: usize  = PAGE_DESC | AF | ATTR_NORMAL | AP_EL1 | UXN;
    pub const PAGE_DEVICE: usize  = PAGE_DESC | AF | ATTR_DEVICE | AP_EL1 | UXN;
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

pub unsafe fn identity_map() {
    for desc in SYS_INFO.lock().efi_ram_layout() {
        let block_ty = desc.ty;
        let addr = desc.phys_start as usize;
        let size = desc.page_count as usize * PAGE_4KIB;

        GLACIER.map_range(addr, addr, size, flags_for_type(block_ty));
    }

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

            pml4 = in(reg) GLACIER.root_table()
        );
    }
}