use crate::{glacier::{AllocParams, GLACIER}, ram::PAGE_4KIB, sysinfo::ramtype, SYS_INFO};

#[derive(Clone, Copy, Debug)]
pub struct MMUConfig {
    pub page_size: PageSize,
    pub va_bits: u8,
    pub pa_bits: u8,
    pub regime: TranslationRegime
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageSize {
    Size4KB,
    Size16KB,
    Size64KB
}

#[derive(Clone, Copy, Debug)]
pub enum TranslationRegime {
    EL1, // EL1&0
    EL2  // EL2 (hypervisor)
}

impl PageSize {
    pub const fn size(&self) -> usize {
        match self {
            Self::Size4KB => 4096,
            Self::Size16KB => 16384,
            Self::Size64KB => 65536
        }
    }

    pub const fn shift(&self) -> u8 {
        match self {
            Self::Size4KB => 12,
            Self::Size16KB => 14,
            Self::Size64KB => 16
        }
    }

    pub const fn tcr_tg0(&self) -> u64 {
        match self {
            Self::Size4KB => 0b00,
            Self::Size16KB => 0b10,
            Self::Size64KB => 0b01
        }
    }

    pub const fn tcr_tg1(&self) -> u64 {
        match self {
            Self::Size4KB => 0b10,
            Self::Size16KB => 0b01,
            Self::Size64KB => 0b11
        }
    }

    pub const fn index_bits(&self) -> u8 {
        match self {
            Self::Size4KB => 9,   // 512 entries
            Self::Size16KB => 11, // 2048 entries
            Self::Size64KB => 13, // 8192 entries
        }
    }

    pub const fn entries_per_table(&self) -> usize {
        1 << self.index_bits()
    }

    pub const fn table_size(&self) -> usize {
        self.entries_per_table() * 8
    }

    pub fn is_supported(&self) -> bool {
        let mut mmfr0: u64;
        unsafe { core::arch::asm!("mrs {}, ID_AA64MMFR0_EL1", out(reg) mmfr0); }

        match self {
            Self::Size4KB => (mmfr0 >> 28) & 0xf != 0xf,
            Self::Size16KB => (mmfr0 >> 20) & 0xf != 0x0,
            Self::Size64KB => (mmfr0 >> 24) & 0xf != 0xf
        }
    }
}

impl MMUConfig {
    pub fn detect() -> Self {
        let mut mmfr0: u64;
        unsafe { core::arch::asm!("mrs {}, ID_AA64MMFR0_EL1", out(reg) mmfr0); }

        let parange = mmfr0 & 0xf;
        let pa_bits = match parange {
            0 => 32,
            1 => 36,
            2 => 40,
            3 => 42,
            4 => 44,
            5 => 48,
            6 => 52,
            _ => 48
        };

        let page_size = if PageSize::Size4KB.is_supported() {
            PageSize::Size4KB
        } else if PageSize::Size16KB.is_supported() {
            PageSize::Size16KB
        } else if PageSize::Size64KB.is_supported() {
            PageSize::Size64KB
        } else {
            panic!("No valid page size supported");
        };

        let va_bits = 48;

        Self {
            page_size, va_bits, pa_bits,
            regime: TranslationRegime::EL1
        }
    }

    pub fn levels(&self) -> u8 {
        let page_shift = self.page_size.shift();
        let index_bits = self.page_size.index_bits();
        let start_bit = self.va_bits - 1;
        let mut levels = 0;
        let mut bit = start_bit;

        while bit >= page_shift {
            levels += 1;
            if bit < index_bits { break; }
            bit = bit.saturating_sub(index_bits);
        }

        return levels;
    }

    pub fn get_index(&self, level: u8, va: u64) -> usize {
        let page_shift = self.page_size.shift();
        let index_bits = self.page_size.index_bits();
        let levels = self.levels();
        if level >= levels { return 0; }

        let shift = page_shift + (levels - level - 1) * index_bits;
        return ((va >> shift) & ((1 << index_bits) - 1)) as usize;
    }

    pub fn tcr_el1(&self) -> u64 {
        let t0sz = 64 - self.va_bits;
        let t1sz = t0sz;
        let mut tcr = 0u64;

        tcr |= (t0sz as u64) << 0;
        tcr |= (t1sz as u64) << 16;

        tcr |= self.page_size.tcr_tg0() << 14;
        tcr |= self.page_size.tcr_tg1() << 30;

        tcr |= 0b01 << 8;   // IRGN0 = Normal WB/WA
        tcr |= 0b01 << 10;  // ORGN0 = Normal WB/WA
        tcr |= 0b11 << 12;  // SH0 = Inner Shareable
        tcr |= 0b01 << 24;  // IRGN1 = Normal WB/WA
        tcr |= 0b01 << 26;  // ORGN1 = Normal WB/WA
        tcr |= 0b11 << 28;  // SH1 = Inner Shareable

        let ips = match self.pa_bits {
            32 => 0b000,
            36 => 0b001,
            40 => 0b010,
            42 => 0b011,
            44 => 0b100,
            48 => 0b101,
            52 => 0b110,
            _ => 0b101
        };
        tcr |= ips << 32;
        return tcr;
    }

    pub fn page_size(&self) -> usize {
        self.page_size.size()
    }
}

#[allow(dead_code)]
pub mod flags {
    // Descriptor type bits [1:0]
    pub const VALID: u64      = 1 << 0;
    pub const TABLE_DESC: u64 = 0b11;      // Table descriptor (levels 0-2)
    pub const BLOCK_DESC: u64 = 0b01;      // Block descriptor (levels 0-2)
    pub const PAGE_DESC: u64  = 0b11;      // Page descriptor (level 3)

    // Memory attributes
    pub const ATTR_IDX_NORMAL: u64 = 0 << 2;
    pub const ATTR_IDX_DEVICE: u64 = 1 << 2;

    // Access permissions
    pub const AP_RW_EL1: u64       = 0b00 << 6;
    pub const AP_RW_ALL: u64       = 0b01 << 6;
    pub const AP_RO_EL1: u64       = 0b10 << 6;
    pub const AP_RO_ALL: u64       = 0b11 << 6;

    // Shareability
    pub const SH_NONE: u64         = 0b00 << 8;
    pub const SH_OUTER: u64        = 0b10 << 8;
    pub const SH_INNER: u64        = 0b11 << 8;

    // Other flags
    pub const AF: u64              = 1 << 10;  // Access Flag
    pub const NG: u64              = 1 << 11;  // Not global
    pub const UXN: u64             = 1 << 54;  // Unprivileged execute never
    pub const PXN: u64             = 1 << 53;  // Privileged execute never

    pub const PAGE_DEFAULT: u64 = PAGE_DESC | AF | ATTR_IDX_NORMAL | SH_INNER | AP_RW_EL1;
    pub const PAGE_NOEXEC: u64  = PAGE_DESC | AF | ATTR_IDX_NORMAL | SH_INNER | AP_RW_EL1 | UXN | PXN;
    pub const PAGE_DEVICE: u64  = PAGE_DESC | AF | ATTR_IDX_DEVICE | SH_NONE | AP_RW_EL1 | UXN | PXN;
}

pub struct PageTableMapper {
    config: MMUConfig,
    root_table: *mut u64
}

impl PageTableMapper {
    pub fn new(config: MMUConfig) -> Self {
        let table_size = config.page_size.table_size();
        let root_table = GLACIER.alloc(
            AllocParams::new(table_size)
                .align(table_size)
                .as_type(ramtype::PAGE_TABLE)
        ).expect("Failed to allocate root page table");

        unsafe { core::ptr::write_bytes(root_table.ptr::<u8>(), 0, table_size); }
        return Self { config, root_table: root_table.ptr() };
    }

    pub fn map_page(&mut self, va: u64, pa: u64, flags: u64) {
        let page_mask = !(self.config.page_size.size() as u64 - 1);
        let va = va & page_mask;
        let pa = pa & page_mask;

        let levels = self.config.levels();
        let mut table = self.root_table;

        for level in 0..levels {
            let index = self.config.get_index(level, va);
            let entry = unsafe { table.add(index) };

            if level == levels - 1 {
                unsafe { *entry = pa | flags; }
                break;
            }

            if unsafe { *entry & flags::VALID == 0 } {
                let table_size = self.config.page_size.table_size();
                let next_table = GLACIER.alloc(
                    AllocParams::new(table_size)
                        .align(table_size)
                        .as_type(ramtype::PAGE_TABLE)
                ).expect("Failed to allocate page table");

                unsafe {
                    core::ptr::write_bytes(next_table.ptr::<u8>(), 0, table_size);
                    *entry = next_table.addr() as u64 | 0b11;
                }
                table = next_table.ptr();
            } else {
                table = unsafe { (*entry & !0xfff) as *mut u64 };
            }
        }
    }

    pub fn root_table(&self) -> *mut u64 {
        self.root_table
    }

    pub fn config(&self) -> &MMUConfig {
        &self.config
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

        let aligned_start = block_start & !(config.page_size() as u64 - 1);
        let aligned_end = (block_end + config.page_size() as u64 - 1) & !(config.page_size() as u64 - 1);

        for phys in (aligned_start..aligned_end).step_by(config.page_size()) {
            mapper.map_page(phys, phys, flags_for_type(block_ty));
        }
    }

    mapper.map_page(0x0900_0000, 0x0900_0000, flags::PAGE_DEVICE); // QEMU UART0
    mapper.map_page(0x0800_0000, 0x0800_0000, flags::PAGE_DEVICE); // GICD
    mapper.map_page(0x0801_0000, 0x0801_0000, flags::PAGE_DEVICE); // GICC

    // Attr0 = Normal memory, Inner/Outer Write-Back Non-transient
    // Attr1 = Device memory nGnRnE
    let mair_el1: u64 = 0xff | (0x00 << 8);
    let tcr_el1 = config.tcr_el1();

    unsafe {
        core::arch::asm!(
            "dsb sy",
            "mrs x0, sctlr_el1",
            "bic x0, x0, #1",
            "msr sctlr_el1, x0",
            "isb",

            "tlbi vmalle1",
            "dsb sy",
            "isb",

            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "msr ttbr0_el1, {ttbr0}",
            "msr ttbr1_el1, xzr",
            "isb",

            "mrs x0, sctlr_el1",
            "orr x0, x0, #(1 << 0)",  // M bit: MMU enable
            "orr x0, x0, #(1 << 2)",  // C bit: Data cache enable
            "orr x0, x0, #(1 << 12)", // I bit: Instruction cache enable
            "msr sctlr_el1, x0",
            "isb",

            "ic iallu",
            "dsb sy",
            "isb",
            mair = in(reg) mair_el1,
            tcr = in(reg) tcr_el1,
            ttbr0 = in(reg) mapper.root_table() as u64
        );
    }
}

pub fn id_map_ptr() -> *const u8 {
    let id_map_ptr: usize;
    unsafe { core::arch::asm!("mrs {}, ttbr0_el1", out(reg) id_map_ptr); }
    return (id_map_ptr & !0xfff) as *const u8;
}