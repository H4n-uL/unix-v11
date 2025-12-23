use crate::{
    arch::rvm::flags,
    ram::physalloc::{AllocParams, PHYS_ALLOC},
    sysinfo::{RAMType, KARGS}
};

use spin::Mutex;

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
        self.size().ilog2() as u8
    }

    pub const fn index_bits(&self) -> u8 {
        self.shift() - size_of::<usize>().ilog2() as u8
    }
}

impl MMUCfg {
    pub fn levels(&self) -> u8 {
        let page_shift = self.page_size.shift();
        let index_bits = self.page_size.index_bits();
        let addr_bits = self.va_bits - page_shift;

        return (addr_bits + index_bits - 1) / index_bits;
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

pub struct Glacier {
    cfg: MMUCfg,
    root_table: usize,
    is_init: bool
}

unsafe impl Send for Glacier {}
unsafe impl Sync for Glacier {}

pub fn flags_for_type(ty: RAMType) -> usize {
    match ty {
        RAMType::Conv => flags::K_RWX,
        RAMType::BootSvcCode => flags::K_RWX,
        RAMType::RtSvcCode => flags::K_ROX,
        RAMType::Kernel => flags::K_RWX,
        RAMType::KernelData => flags::K_RWX,
        RAMType::KernelPTable => flags::K_RWX,
        RAMType::MMIO => flags::D_RW,
        _ => flags::K_RWX
    }
}

impl Glacier {
    const fn empty() -> Self {
        Self {
            cfg: MMUCfg {
                page_size: PageSize::Size4kiB,
                va_bits: 0,
                pa_bits: 0
            },
            root_table: 0,
            is_init: false
        }
    }

    pub fn init(&mut self) {
        if self.is_init { return; }
        self.cfg = MMUCfg::detect();
        let table_size = self.cfg.page_size.size();
        let root_table = PHYS_ALLOC.alloc(
            AllocParams::new(table_size)
                .align(table_size)
                .as_type(RAMType::KernelPTable)
        ).expect("Failed to allocate root page table");

        unsafe { root_table.ptr::<u8>().write_bytes(0, table_size); }
        self.root_table = root_table.addr();
        self.is_init = true;

        for desc in KARGS.efi_ram_layout() {
            let block_ty = desc.ty;
            let addr = desc.phys_start as usize;
            let size = desc.page_count as usize * 0x1000;

            self.map_range(addr, addr, size, flags_for_type(block_ty));
        }
        self.identity_map();
    }

    pub fn map_page(&self, va: usize, pa: usize, flags: usize) {
        if !self.is_init { return; }
        let page_mask = !(self.cfg.page_size() - 1);
        let va = va & page_mask;
        let pa = pa & page_mask;

        let levels = self.cfg.levels();
        let mut table = self.root_table;

        for level in 0..levels {
            let index = self.cfg.get_index(level, va);
            let entry = unsafe { (table as *mut usize).add(index) };

            if level == levels - 1 {
                unsafe { *entry = pa | flags; }
                break;
            }

            if unsafe { *entry & flags::VALID == 0 } {
                let table_size = self.cfg.page_size.size();
                let next_table = PHYS_ALLOC.alloc(
                    AllocParams::new(table_size)
                        .align(table_size)
                        .as_type(RAMType::KernelPTable)
                ).expect("Failed to allocate page table");

                unsafe {
                    next_table.ptr::<u8>().write_bytes(0, table_size);
                    *entry = next_table.addr() | flags::NEXT;
                }
                table = next_table.ptr::<()>() as usize;
            } else {
                table = unsafe { *entry & self.cfg.page_size.addr_mask() };
            }
        }
    }

    pub fn unmap_page(&self, va: usize) {
        if !self.is_init { return; }
        let va = va & !(self.cfg.page_size() - 1);
        let _ = self.unmap_rec(self.root_table, va, 0);
    }

    fn unmap_rec(&self, table: usize, va: usize, level: u8) -> bool {
        let is_tbl_null = |tbl: usize| {
            (0..1 << self.cfg.page_size.index_bits())
                .all(|i| unsafe { *(tbl as *const usize).add(i) == 0 })
        };

        let index = self.cfg.get_index(level, va);
        let entry = unsafe { (table as *mut usize).add(index) };

        if level == self.cfg.levels() - 1 {
            unsafe { *entry = 0; }
            return is_tbl_null(table);
        }

        if unsafe { *entry & flags::VALID == 0 } {
            return false;
        }

        let child = unsafe { *entry & self.cfg.page_size.addr_mask() };

        if self.unmap_rec(child, va, level + 1) {
            unsafe {
                *entry = 0;
                PHYS_ALLOC.free_raw(child as *mut u8, self.cfg.page_size.size());
            }
            return is_tbl_null(table);
        }
        return false;
    }

    pub fn map_range(&self, va: usize, pa: usize, size: usize, flags: usize) {
        if !self.is_init { return; }
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

    pub fn unmap_range(&self, va: usize, size: usize) {
        if !self.is_init { return; }
        let page_size = self.cfg.page_size();
        let page_mask = !(page_size - 1);

        let va_start = va & page_mask;
        let va_end = (va + size + page_size - 1) & page_mask;

        for va in (va_start..va_end).step_by(page_size) {
            self.unmap_page(va);
        }
    }

    pub fn root_table(&self) -> *mut usize {
        return self.root_table as *mut usize;
    }

    pub fn cfg(&self) -> MMUCfg {
        return self.cfg;
    }
}

pub static GLACIER: GlacierGlob = GlacierGlob::empty();

pub struct GlacierGlob(pub Mutex<Glacier>);
impl GlacierGlob {
    const fn empty() -> Self {
        return Self(Mutex::new(Glacier::empty()));
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

    // pub fn unmap_page(&self, va: usize) {
    //     self.0.lock().unmap_page(va);
    // }

    pub fn unmap_range(&self, va: usize, size: usize) {
        self.0.lock().unmap_range(va, size);
    }

    pub fn root_table(&self) -> *mut usize {
        return self.0.lock().root_table();
    }

    pub fn cfg(&self) -> MMUCfg {
        return self.0.lock().cfg();
    }
}
