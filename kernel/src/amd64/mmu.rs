use crate::{ram::{physalloc::{AllocParams, PHYS_ALLOC}, PAGE_4KIB}, sysinfo::ramtype, SYS_INFO};

#[derive(Clone, Copy, Debug)]
pub struct MMUConfig {
    pub page_size: PageSize,
    pub va_bits: u8,
    pub pa_bits: u8,
    pub levels: u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageSize {
    Size4kiB = 4096
}

impl PageSize {
    pub const fn size(&self) -> usize { *self as usize }
    pub const fn shift(&self) -> u8 { 12 }
    pub const fn index_bits(&self) -> u8 { 9 }
    pub const fn entries_per_table(&self) -> usize { 512 }
    pub const fn table_size(&self) -> usize { 4096 }
    pub fn is_supported(&self) -> bool { true }
}

impl MMUConfig {
    pub fn detect() -> Self {
        return Self {
            page_size: PageSize::Size4kiB,
            va_bits: 48,
            pa_bits: 52,
            levels: 4
        };
    }

    pub fn get_index(&self, level: u8, va: u64) -> usize {
        let shift = match level {
            0 => 39, // PML4
            1 => 30, // PDPT
            2 => 21, // PD
            3 => 12, // PT
            _ => unreachable!()
        };
        return ((va >> shift) & 0x1ff) as usize;
    }
}

pub mod flags {
    pub const VALID: u64       = 1 << 0;  // Present bit
    pub const TABLE_DESC: u64  = 0x03;    // Present + Writable for non-leaf
    pub const PAGE_DESC: u64   = 0x03;    // Present + Writable for leaf

    pub const ATTR_NORMAL: u64 = 0;       // Normal memory (cacheable)
    pub const ATTR_DEVICE: u64 = 1 << 4;  // Device memory (PCD bit)

    pub const AP_RW_EL1: u64   = 0;       // Kernel R/W (no USER bit)
    pub const AP_RW_ALL: u64   = 1 << 2;  // User accessible

    pub const AF: u64          = 1 << 5;  // Accessed flag
    pub const UXN: u64         = 1 << 63; // No execute (NX bit)
    pub const PXN: u64         = 1 << 63; // Same as UXN for x86

    pub const PAGE_DEFAULT: u64 = PAGE_DESC | AF | ATTR_NORMAL | AP_RW_EL1;
    pub const PAGE_NOEXEC: u64  = PAGE_DESC | AF | ATTR_NORMAL | AP_RW_EL1 | UXN;
    pub const PAGE_DEVICE: u64  = PAGE_DESC | AF | ATTR_DEVICE | AP_RW_EL1 | UXN;
}

pub struct PageTableMapper {
    config: MMUConfig,
    root_table: *mut u64
}

impl PageTableMapper {
    pub fn new(config: MMUConfig) -> Self {
        let table_size = config.page_size.table_size();
        let root_table = PHYS_ALLOC.alloc(
            AllocParams::new(table_size)
                .align(table_size)
                .as_type(ramtype::PAGE_TABLE)
        ).expect("Failed to allocate root page table");

        unsafe { core::ptr::write_bytes(root_table.ptr::<u8>(), 0, table_size); }
        Self { config, root_table: root_table.ptr() }
    }

    pub fn map_page(&mut self, va: u64, pa: u64, flags: u64) {
        let va = va & !0xfff;
        let pa = pa & !0xfff;

        let mut table = self.root_table;

        for level in 0..self.config.levels {
            let index = self.config.get_index(level, va);
            let entry = unsafe { table.add(index) };

            if level == self.config.levels - 1 {
                unsafe { *entry = pa | flags; }
                break;
            }

            if unsafe { *entry & flags::VALID == 0 } {
                let table_size = self.config.page_size.table_size();
                let next_table = PHYS_ALLOC.alloc(
                    AllocParams::new(table_size)
                        .align(table_size)
                        .as_type(ramtype::PAGE_TABLE)
                ).expect("Failed to allocate page table");

                unsafe {
                    core::ptr::write_bytes(next_table.ptr::<u8>(), 0, table_size);
                    *entry = next_table.addr() as u64 | flags::TABLE_DESC;
                }
                table = next_table.ptr();
            } else {
                table = unsafe { (*entry & !0xfff) as *mut u64 };
            }
        }
    }

    pub fn root_table(&self) -> *mut u64 {
        return self.root_table;
    }

    pub fn config(&self) -> &MMUConfig {
        return &self.config;
    }
}

pub fn flags_for_type(ty: u32) -> u64 {
    use flags::*;
    match ty {
        ramtype::CONVENTIONAL => PAGE_DEFAULT,
        ramtype::BOOT_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::RUNTIME_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::KERNEL => PAGE_DEFAULT,
        ramtype::KERNEL_DATA => PAGE_NOEXEC,
        ramtype::PAGE_TABLE => PAGE_NOEXEC,
        ramtype::MMIO => PAGE_DEVICE,
        _ => PAGE_NOEXEC
    }
}

pub unsafe fn identity_map() {
    let config = MMUConfig::detect();
    let mut mapper = PageTableMapper::new(config);

    for desc in SYS_INFO.lock().efi_ram_layout() {
        let block_ty = desc.ty;
        let block_start = desc.phys_start;
        let block_end = block_start + desc.page_count * PAGE_4KIB as u64;

        for phys in (block_start..block_end).step_by(PAGE_4KIB) {
            mapper.map_page(phys, phys, flags_for_type(block_ty));
        }
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

            pml4 = in(reg) mapper.root_table() as u64
        );
    }
}

pub fn id_map_ptr() -> *const u8 {
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3); }
    (cr3 & !0xfff) as *const u8
}