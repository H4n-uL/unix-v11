use crate::{ram::{physalloc::{AllocParams, PHYS_ALLOC}, PAGE_4KIB}, sysinfo::ramtype, SYS_INFO};

#[derive(Clone, Copy, Debug)]
pub struct MMUConfig {
    pub page_size: PageSize,
    pub va_bits: u8,
    pub pa_bits: u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageSize {
    Size4kiB  = 4096,
    Size16kiB = 16384,
    Size64kiB = 65536
}

impl PageSize {
    pub const fn size(&self) -> usize {
        *self as usize
    }

    pub const fn addr_mask(&self) -> u64 {
        !(self.size() - 1) as u64
    }

    pub const fn shift(&self) -> u8 {
        match self {
            Self::Size4kiB  => 12,
            Self::Size16kiB => 14,
            Self::Size64kiB => 16
        }
    }

    pub const fn index_bits(&self) -> u8 {
        match self {
            Self::Size4kiB  => 9,  //  512 entries
            Self::Size16kiB => 11, // 2048 entries
            Self::Size64kiB => 13  // 8192 entries
        }
    }

    pub const fn entries_per_table(&self) -> usize {
        1 << self.index_bits()
    }

    pub const fn table_size(&self) -> usize {
        self.entries_per_table() * 8
    }

    pub fn from_tcr_tg(tg_bits: usize) -> Option<Self> {
        match tg_bits {
            0b00 => Some(Self::Size4kiB),
            0b01 => Some(Self::Size64kiB),
            0b10 => Some(Self::Size16kiB),
            _ => None
        }
    }
}

impl MMUConfig {
    pub fn detect() -> Self {
        let mut tcr_el1: usize;
        unsafe { core::arch::asm!("mrs {}, tcr_el1", out(reg) tcr_el1); }

        let t0sz = tcr_el1 & 0x3f;
        let va_bits = 64 - t0sz as u8;

        let tg0 = (tcr_el1 >> 14) & 0x3;
        let page_size = PageSize::from_tcr_tg(tg0)
            .expect("Invalid TG0 value in TCR_EL1");

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

        return Self { page_size, va_bits, pa_bits };
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
        if level >= levels { unreachable!(); }

        let shift = page_shift + (levels - level - 1) * index_bits;
        return ((va >> shift) & ((1 << index_bits) - 1)) as usize;
    }

    pub fn tcr_el1(&self) -> usize {
        let mut tcr: usize;
        unsafe { core::arch::asm!("mrs {}, tcr_el1", out(reg) tcr); }
        tcr &= 0xc03fc03f; // T0SZ, T1SZ, TG0, TG1

        tcr |= 0b01 << 8;  // IRGN0 = Normal WB/WA
        tcr |= 0b01 << 10; // ORGN0 = Normal WB/WA
        tcr |= 0b11 << 12; // SH0 = Inner Shareable
        tcr |= 0b01 << 24; // IRGN1 = Normal WB/WA
        tcr |= 0b01 << 26; // ORGN1 = Normal WB/WA
        tcr |= 0b11 << 28; // SH1 = Inner Shareable

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
        return self.page_size.size();
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
        let root_table = PHYS_ALLOC.alloc(
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
                table = unsafe { (*entry & self.config.page_size.addr_mask()) as *mut u64 };
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
        _ => PAGE_NOEXEC,
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

    unsafe {
        core::arch::asm!(
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
            tcr = in(reg) config.tcr_el1(),
            ttbr0 = in(reg) mapper.root_table() as u64
        );
    }
}

pub fn id_map_ptr() -> *const u8 {
    let id_map_ptr: usize;
    unsafe { core::arch::asm!("mrs {}, ttbr0_el1", out(reg) id_map_ptr); }
    return (id_map_ptr & !0xfff) as *const u8;
}