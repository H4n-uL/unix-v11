use crate::{
    arch::rvm::flags,
    kargs::{NON_RAM, RAMType, efi_ram_layout},
    ram::{mutex::IntRwLock, physalloc::{AllocParams, PHYS_ALLOC}}
};

use spin::{Once, RwLock};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BPage {
    Size4kiB  = 12,
    Size16kiB = 14,
    Size64kiB = 16
}

#[derive(Clone, Copy, Debug)]
pub struct RvmCfg {
    pub psz: BPage,
    pub va_bits: u8,
    pub pa_bits: u8
}

impl BPage {
    pub const fn shift(self) -> u8 {
        return self as u8;
    }

    pub const fn size(self) -> usize {
        return 1usize << self.shift();
    }

    pub const fn addr_mask(self) -> usize {
        return !(self.size() - 1);
    }

    pub const fn index_bits(self) -> u8 {
        return self.shift() - size_of::<usize>().ilog2() as u8;
    }
}

impl RvmCfg {
    pub fn levels(&self) -> u8 {
        let page_shift = self.psz.shift();
        let index_bits = self.psz.index_bits();
        let addr_bits = self.va_bits - page_shift;
        return (addr_bits + index_bits - 1) / index_bits;
    }

    pub fn shift(&self, level: u8) -> u8 {
        let page_shift = self.psz.shift();
        let index_bits = self.psz.index_bits();
        let levels = self.levels();
        if level >= levels { unreachable!(); }

        return page_shift + (levels - level - 1) * index_bits;
    }

    pub fn get_index(&self, level: u8, va: usize) -> usize {
        return (va >> self.shift(level)) & (self.ent_cnt(level) - 1);
    }

    pub fn ent_cnt(&self, level: u8) -> usize {
        return 1usize << if level == 0 {
            self.va_bits - self.shift(level)
        } else {
            self.psz.index_bits()
        };
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum GlacierErr {
    Failed2Alloc
}

pub struct Glacier {
    root_table: usize,
    is_init: bool
}

unsafe impl Send for Glacier {}
unsafe impl Sync for Glacier {}

impl Glacier {
    const fn empty() -> Self {
        return Self {
            root_table: 0,
            is_init: false
        };
    }

    unsafe fn init(&mut self) {
        // SAFETY: As this function is private, the is_init flag may be omitted.
        // if self.is_init { return; }

        let table_size = self.cfg().psz.size();
        let root_table = PHYS_ALLOC.alloc(
            AllocParams::new(table_size)
                .align(table_size)
                .as_type(RAMType::KernelPTable)
        ).expect("Failed to allocate root page table");

        unsafe { root_table.ptr::<u8>().write_bytes(0, table_size); }
        self.root_table = root_table.addr();
        self.is_init = true;
    }

    pub fn new() -> Self {
        let mut new = Self::empty();

        unsafe {
            new.init();

            let page_size = new.cfg().psz.size();
            let krvm_root = GLACIER.read().root_table;
            let new_root = new.root_table;

            let hihalf_idx = new.cfg().ent_cnt(0) >> 1;

            (new_root as *mut u8)
                .copy_from(krvm_root as *const u8, page_size);
            (new_root as *mut u8)
                .write_bytes(0, hihalf_idx * size_of::<usize>());
        }

        return new;
    }

    pub fn map_page(&mut self, va: usize, pa: usize, flags: usize) -> Result<(), GlacierErr> {
        // SAFETY: As the `empty` and `init` functions are private, the is_init flag may be omitted.
        // if !self.is_init { return; }

        let page_mask = !(self.cfg().psz.size() - 1);
        let va = va & page_mask;
        let pa = pa & page_mask;

        let levels = self.cfg().levels();
        let mut table = self.root_table;

        for level in 0..levels {
            let index = self.cfg().get_index(level, va);
            let entry = unsafe { (table as *mut usize).add(index) };

            if level == levels - 1 {
                unsafe { *entry = pa | flags; }
                break;
            }

            if unsafe { *entry & flags::VALID == 0 } {
                let table_size = self.cfg().psz.size();
                let next_table = PHYS_ALLOC.alloc(
                    AllocParams::new(table_size)
                        .align(table_size)
                        .as_type(RAMType::KernelPTable)
                ).ok_or(GlacierErr::Failed2Alloc)?;

                unsafe {
                    next_table.ptr::<u8>().write_bytes(0, table_size);
                    *entry = next_table.addr() | flags::NEXT;
                }
                table = next_table.ptr::<()>() as usize;
            } else {
                table = unsafe { *entry & self.cfg().psz.addr_mask() };
            }
        }

        self.flush(va);
        return Ok(());
    }

    pub fn unmap_page(&mut self, va: usize) {
        // SAFETY: As the `empty` and `init` functions are private, the is_init flag may be omitted.
        // if !self.is_init { return; }

        let va = va & !(self.cfg().psz.size() - 1);
        let _ = self.unmap_rec(self.root_table, va, 0);
    }

    fn unmap_rec(&self, table: usize, va: usize, level: u8) -> bool {
        let entries = self.cfg().ent_cnt(level);
        let is_tbl_null = || (0..entries).all(|i| unsafe {
            *(table as *const usize).add(i) == 0
        });

        let index = self.cfg().get_index(level, va);
        let entry = unsafe { (table as *mut usize).add(index) };

        if level == self.cfg().levels() - 1 {
            unsafe { *entry = 0; }
            self.flush(va);
            return is_tbl_null();
        }

        if unsafe { *entry & flags::VALID == 0 } {
            return false;
        }

        let child = unsafe { *entry & self.cfg().psz.addr_mask() };

        if self.unmap_rec(child, va, level + 1) {
            unsafe {
                *entry = 0;
                PHYS_ALLOC.free_raw(child as *mut u8, self.cfg().psz.size());
            }
            self.flush(va);
            return is_tbl_null();
        }
        return false;
    }

    pub fn map_range(&mut self, va: usize, pa: usize, size: usize, flags: usize) -> Result<(), GlacierErr> {
        // SAFETY: As the `empty` and `init` functions are private, the is_init flag may be omitted.
        // if !self.is_init { return; }

        let page_size = self.cfg().psz.size();
        let page_mask = !(page_size - 1);

        let pa_start = pa & page_mask;
        let va_start = va & page_mask;
        let va_end = (va + size + page_size - 1) & page_mask;

        for va in (va_start..va_end).step_by(page_size) {
            let pa = pa_start + (va - va_start);
            self.map_page(va, pa, flags)?;
        }

        return Ok(());
    }

    pub fn unmap_range(&mut self, va: usize, size: usize) {
        // SAFETY: As the `empty` and `init` functions are private, the is_init flag may be omitted.
        // if !self.is_init { return; }

        let page_size = self.cfg().psz.size();
        let page_mask = !(page_size - 1);

        let va_start = va & page_mask;
        let va_end = (va + size + page_size - 1) & page_mask;

        for va in (va_start..va_end).step_by(page_size) {
            self.unmap_page(va);
        }
    }

    pub fn get_pa(&self, va: usize) -> Option<usize> {
        // SAFETY: As the `empty` and `init` functions are private, the is_init flag may be omitted.
        // if !self.is_init { return None; }

        let page_mask = !(self.cfg().psz.size() - 1);
        let va = va & page_mask;

        let levels = self.cfg().levels();
        let mut table = self.root_table;

        for level in 0..levels {
            let index = self.cfg().get_index(level, va);
            let entry = unsafe { *((table as *const usize).add(index)) };

            if entry & flags::VALID == 0 {
                return None;
            }

            if level == levels - 1 {
                return Some(entry & self.cfg().psz.addr_mask());
            } else {
                table = entry & self.cfg().psz.addr_mask();
            }
        }

        return None;
    }

    pub fn root_table(&self) -> *mut usize {
        return self.root_table as *mut usize;
    }

    pub fn cfg(&self) -> RvmCfg {
        unsafe { return *G_CFG.get_unchecked(); }
    }
}

impl Drop for Glacier {
    fn drop(&mut self) {
        if !self.is_init { return; }
        self._drop(self.root_table, 0);
    }
}

impl Glacier {
    fn _drop(&self, table: usize, level: u8) {
        let mut entries = self.cfg().ent_cnt(level);
        if level == 0 { entries >>= 1; }

        for i in 0..entries {
            let entry = unsafe { *((table as *const usize).add(i)) };

            if entry & flags::VALID != 0 {
                if level < self.cfg().levels() - 1 {
                    let child = entry & self.cfg().psz.addr_mask();
                    self._drop(child, level + 1);
                }
            }
        }

        unsafe {
            PHYS_ALLOC.free_raw(table as *mut u8, self.cfg().psz.size());
        }
    }
}

pub static G_CFG: Once<RvmCfg> = Once::new();
pub static GLACIER: IntRwLock<RwLock<()>, Glacier> = IntRwLock::new(Glacier::empty());

#[inline(always)]
pub fn hihalf() -> usize {
    unsafe { return !0 << (G_CFG.get_unchecked().va_bits - 1); }
}

#[inline(always)]
pub fn page_size() -> usize {
    unsafe { return G_CFG.get_unchecked().psz.size(); }
}

pub fn init() {
    let mut glacier = GLACIER.write();
    unsafe { glacier.init(); }

    for desc in efi_ram_layout() {
        let block_ty = desc.ty;
        let addr = desc.phys_start as usize;
        let size = desc.page_count as usize * 0x1000;

        if block_ty == RAMType::MMIO || block_ty == RAMType::MMIOPortSpace {
            glacier.map_range(addr, addr, size, flags::D_RW).unwrap();
            continue;
        }

        if block_ty == RAMType::Reserved {
            continue;
        }

        glacier.map_range(addr, addr, size, flags::K_RWX).unwrap();
    }

    glacier.identity_map();
}

pub fn remap() {
    let mut glacier = GLACIER.write();

    for desc in efi_ram_layout() {
        let block_ty = desc.ty;
        let addr = desc.phys_start as usize;
        let size = desc.page_count as usize * 0x1000;
        if NON_RAM.contains(&block_ty) {
            continue;
        }

        glacier.map_range(addr, addr, size, flags::K_RWO).unwrap();
    }
}
