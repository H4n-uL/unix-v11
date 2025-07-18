use spin::Mutex;

use crate::{
    arch::mmu::flags,
    ram::physalloc::{AllocParams, PHYS_ALLOC},
    sysinfo::ramtype
};

#[derive(Clone, Copy, Debug)]
pub struct MMUCfg {
    pub page_size: PageSize,
    pub va_bits: u8,
    pub pa_bits: u8
}

#[repr(u32)]
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

    pub const fn addr_mask(&self) -> usize {
        !(self.size() - 1)
    }

    pub const fn shift(&self) -> u8 {
        match self {
            Self::Size4kiB  => 12,
            Self::Size16kiB => 14,
            Self::Size64kiB => 16
        }
    }

    pub const fn index_bits(&self) -> u8 {
        self.shift() - (usize::BITS / u8::BITS).ilog2() as u8
    }

    pub const fn entries_per_table(&self) -> usize {
        1 << self.index_bits()
    }

    pub const fn table_size(&self) -> usize {
        self.entries_per_table() * 8
    }
}

impl MMUCfg {
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

    pub fn get_index(&self, level: u8, va: usize) -> usize {
        let ps = self.page_size;
        let page_shift = ps.shift();
        let index_bits = ps.index_bits();
        let levels = self.levels();
        if level >= levels { unreachable!(); }

        let shift = page_shift + (levels - level - 1) * index_bits;
        return (va >> shift) & ((1 << index_bits) - 1);
    }

    pub fn page_size(&self) -> usize {
        return self.page_size.size();
    }
}

struct GlacierData {
    cfg: MMUCfg,
    root_table: *mut usize,
    is_init: bool
}

unsafe impl Send for GlacierData {}
unsafe impl Sync for GlacierData {}

impl GlacierData {
    const fn empty() -> Self {
        Self {
            cfg: MMUCfg {
                page_size: PageSize::Size4kiB,
                va_bits: 0,
                pa_bits: 0
            },
            root_table: core::ptr::null_mut(),
            is_init: false
        }
    }

    fn init(&mut self) {
        if self.is_init { return; }
        self.cfg = MMUCfg::detect();
        let table_size = self.cfg.page_size.table_size();
        let root_table = PHYS_ALLOC.alloc(
            AllocParams::new(table_size)
                .align(table_size)
                .as_type(ramtype::KERNEL_PAGE_TABLE)
        ).expect("Failed to allocate root page table");

        unsafe { core::ptr::write_bytes(root_table.ptr::<u8>(), 0, table_size); }
        self.root_table = root_table.ptr();
        self.is_init = true;
    }

    fn map_page(&mut self, va: usize, pa: usize, flags: usize) {
        if !self.is_init { self.init(); }
        let page_mask = !(self.cfg.page_size() - 1);
        let va = va & page_mask;
        let pa = pa & page_mask;

        let levels = self.cfg.levels();
        let mut table = self.root_table;

        for level in 0..levels {
            let index = self.cfg.get_index(level, va);
            let entry = unsafe { table.add(index) };

            if level == levels - 1 {
                unsafe { *entry = pa | flags; }
                break;
            }

            if unsafe { *entry & flags::VALID == 0 } {
                let table_size = self.cfg.page_size.table_size();
                let next_table = PHYS_ALLOC.alloc(
                    AllocParams::new(table_size)
                        .align(table_size)
                        .as_type(ramtype::KERNEL_PAGE_TABLE)
                ).expect("Failed to allocate page table");

                unsafe {
                    core::ptr::write_bytes(next_table.ptr::<u8>(), 0, table_size);
                    *entry = next_table.addr() | flags::TABLE_DESC;
                }
                table = next_table.ptr();
            } else {
                table = unsafe { (*entry & self.cfg.page_size.addr_mask()) as *mut usize };
            }
        }
    }

    fn map_range(&mut self, va: usize, pa: usize, size: usize, flags: usize) {
        if !self.is_init { self.init(); }
        let page_size = self.cfg.page_size();
        let page_mask = !(page_size - 1);

        let pa_start = pa & page_mask;
        let va_start = va & page_mask;
        let va_end = (va + size + page_size - 1) & page_mask;

        for va in (va_start..va_end).step_by(page_size) {
            let pa = pa_start + (va - va_start);
            self.map_page(va, pa, flags);
        }
    }

    fn root_table(&self) -> *mut usize {
        return self.root_table;
    }

    fn cfg(&self) -> MMUCfg {
        return self.cfg;
    }
}

pub static GLACIER: Glacier = Glacier::empty();

pub struct Glacier(Mutex<GlacierData>);
impl Glacier {
    const fn empty() -> Self {
        return Self(Mutex::new(GlacierData::empty()));
    }

    pub fn init(&self) {
        self.0.lock().init();
    }

    pub fn map_page(&self, va: usize, pa: usize, flags: usize) {
        self.0.lock().map_page(va, pa, flags);
    }

    pub fn map_range(&self, va: usize, pa: usize, size: usize, flags: usize) {
        self.0.lock().map_range(va, pa, size, flags);
    }

    pub fn root_table(&self) -> *mut usize {
        return self.0.lock().root_table();
    }

    pub fn cfg(&self) -> MMUCfg {
        return self.0.lock().cfg();
    }
}