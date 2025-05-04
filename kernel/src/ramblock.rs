use crate::{arch, ember::{ramtype, Ember}, ram::PAGE_4KIB};
use spin::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlock {
    addr: *const u8,
    size: usize,
    ty: u32,
    used: bool
}

impl RAMBlock {
    pub fn new(addr: *const u8, size: usize, ty: u32, used: bool) -> Self {
        return RAMBlock { addr, size, ty, used };
    }
    pub fn addr(&self) -> usize { self.addr as usize }
    pub fn ptr(&self) -> *mut u8 { self.addr as *mut u8 }
    pub fn size(&self) -> usize { self.size }
    pub fn ty(&self) -> u32 { self.ty }
    pub fn used(&self) -> bool  { self.used }
    pub fn set_ty(&mut self, ty: u32) { self.ty = ty; }
}

#[repr(C)]
#[derive(Debug)]
pub struct RAMBlockManager {
    blocks: *mut Option<RAMBlock>,
    is_init: bool,
    count: usize,
    max: usize
}

pub const BASE_RAMBLOCK_SIZE: usize = 128;
pub static RAM_BLOCKS: Mutex<[Option<RAMBlock>; BASE_RAMBLOCK_SIZE]> = Mutex::new([None; BASE_RAMBLOCK_SIZE]);
pub static RAM_BLOCK_MANAGER: Mutex<RAMBlockManager> = Mutex::new(RAMBlockManager {
    blocks: &raw const RAM_BLOCKS as *mut Option<RAMBlock>,
    is_init: false, count: 0,
    max: BASE_RAMBLOCK_SIZE,
});
unsafe impl Send for RAMBlock {}
unsafe impl Sync for RAMBlock {}
unsafe impl Send for RAMBlockManager {}
unsafe impl Sync for RAMBlockManager {}

impl RAMBlockManager {
    pub unsafe fn ptr(&self) -> *mut Option<RAMBlock> { self.blocks }
    pub unsafe fn max(&self) -> usize { self.max }

    pub fn init(&mut self, ember: &mut Ember) {
        ember.sort_ram_layout_by(|desc| desc.page_count);
        if self.is_init { return; } self.count = 0;
        for desc in ember.efi_ram_layout().iter().rev() {
            if desc.ty == ramtype::CONVENTIONAL {
                let size = desc.page_count as usize * PAGE_4KIB;
                let addr = desc.phys_start as *const u8;
                self.add(addr, size, desc.ty, false);
            }
        }
        for desc in ember.efi_ram_layout() {
            if desc.ty != ramtype::CONVENTIONAL {
                let size = desc.page_count as usize * PAGE_4KIB;
                let addr = desc.phys_start as *const u8;
                self.add(addr, size, desc.ty, true);
            }
        }
    }

    pub fn identity_map_size(&self) -> usize {
        return self.blocks().iter().flatten()
            .map(|block| block.addr() + block.size())
            .max().unwrap_or(0);
    }

    pub fn count_filter(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.blocks().iter().filter(|block| block.is_some() && filter(block.as_ref().unwrap()))
            .map(|block| block.as_ref().unwrap().size()).sum();
    }

    pub fn available(&self) -> usize {
        return self.count_filter(|block| !block.used() && block.ty() == ramtype::CONVENTIONAL);
    }

    pub fn total(&self) -> usize {
        return self.count_filter(|_| true);
    }

    pub fn from_addr_mut(&mut self, addr: *const u8) -> Option<&mut RAMBlock> {
        let pos = self.blocks_mut().iter().position(|block| {
            if let Some(block) = block { block.addr <= addr && block.addr() + block.size > addr as usize }
            else { false }
        });

        return if let Some(idx) = pos { self.blocks_mut()[idx].as_mut() } else { None };
    }

    pub fn blocks(&self) -> &[Option<RAMBlock>] {
        unsafe { core::slice::from_raw_parts(self.blocks, self.count) }
    }

    fn blocks_mut(&mut self) -> &mut [Option<RAMBlock>] {
        unsafe { core::slice::from_raw_parts_mut(self.blocks, self.count) }
    }

    pub fn sort(&mut self) {
        use crate::sort::HeaplessSort;
        self.blocks_mut().sort_noheap_by(|a, b| match (a, b) {
            (Some(a), Some(b)) => a.addr.cmp(&b.addr),
            (Some(_), None) => core::cmp::Ordering::Less,
            (None, Some(_)) => core::cmp::Ordering::Greater,
            (None, None) => core::cmp::Ordering::Equal,
        });
    }

    pub fn find_ram(&self, size: usize, ty: u32) -> Option<*mut u8> {
        return self.blocks().iter().flatten()
            .find(|block| !block.used && block.size >= size && block.ty == ty)
            .map(|block| block.addr as *mut u8);
    }

    fn add(&mut self, addr: *const u8, size: usize, ty: u32, used: bool) {
        if self.count >= self.max { self.expand(self.max * 2); }
        let idx = self.count; self.count += 1;
        let blocks = self.blocks_mut();
        blocks[idx] = Some(RAMBlock::new(addr, size, ty, used));

        for i in (1..=idx).rev() {
            if let (Some(current), Some(prev)) = (blocks[i], blocks[i - 1]) {
                if current.addr < prev.addr { blocks.swap(i, i - 1); }
                else { break; }
            }
            else { break; }
        }
    }

    pub fn reserve_at_as(&mut self, addr: *const u8, size: usize, ty: u32, as_ty: u32, used: bool) -> Option<*mut u8> {
        let target_idx = self.blocks_mut().iter().position(|block_opt| {
            if let Some(block) = block_opt {
                !block.used && block.ty == ty &&
                addr >= block.addr && addr as usize + size <= block.addr() + block.size
            }
            else { false }
        });

        if let Some(idx) = target_idx {
            let block_opt = &mut self.blocks_mut()[idx];
            let block = *block_opt.as_ref().unwrap();
            let block_end = block.addr() + block.size;
            let before_size = addr as usize - block.addr();
            let after_size = block_end - (addr as usize + size);

            *block_opt = Some(RAMBlock::new(addr, size, as_ty, used));
            if before_size > 0 { self.add(block.addr, before_size, block.ty, block.used); }
            if after_size > 0  { self.add(unsafe { addr.add(size) },  after_size, block.ty, block.used); }
            return Some(addr as *mut u8);
        }

        arch::serial_puts("Tried to allocate unknown block\n");
        return None;
    }

    pub fn reserve_as(&mut self, size: usize, ty: u32, as_ty: u32, used: bool) -> Option<*mut u8> {
        return self.find_ram(size, ty)
            .and_then(|addr| self.reserve_at_as(addr, size, ty, as_ty, used));
    }

    pub fn alloc_at(&mut self, addr: *const u8, size: usize, ty: u32) -> Option<*mut u8> {
        return self.reserve_at_as(addr, size, ty, ty, true);
    }

    pub fn alloc(&mut self, size: usize, ty: u32) -> Option<*mut u8> {
        return self.find_ram(size, ty)
            .and_then(|addr| self.alloc_at(addr, size, ty));
    }

    pub fn free(&mut self, addr: *const u8) {
        let found = self.blocks_mut().iter_mut().flatten()
            .find(|block| block.addr <= addr && block.addr() + block.size > addr as usize);
        if let Some(block) = found { block.used = false; }
    }

    pub fn expand(&mut self, new_max: usize) {
        if new_max <= self.max { return; }

        let manager_size = new_max * core::mem::size_of::<Option<RAMBlock>>();
        let old_blocks_ptr = self.blocks;
        let new_blocks_ptr = self.find_ram(manager_size, ramtype::CONVENTIONAL).unwrap() as *mut Option<RAMBlock>;
        unsafe { core::ptr::copy(old_blocks_ptr, new_blocks_ptr, self.count); }
        (self.blocks, self.max) = (new_blocks_ptr, new_max);
        self.alloc_at(new_blocks_ptr as *mut u8, manager_size, ramtype::CONVENTIONAL);
        self.free(old_blocks_ptr as *const u8);
    }
}