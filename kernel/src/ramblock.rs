use crate::{ember::ramtype, ram::{align_up, PAGE_4KIB}, EMBER};
use spin::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlock {
    addr: *const u8,
    size: usize,
    ty: u32,
    valid: bool,
    used: bool
}

impl RAMBlock {
    pub fn new(addr: *const u8, size: usize, ty: u32, used: bool) -> Self {
        return RAMBlock { addr, size, ty, valid: true, used };
    }
    pub const fn new_invalid() -> Self {
        return RAMBlock { addr: core::ptr::null(), size: 0, ty: 0, valid: false, used: false };
    }

    pub fn addr(&self) -> usize { self.addr as usize }
    pub fn ptr(&self) -> *mut u8 { self.addr as *mut u8 }
    pub fn size(&self) -> usize { self.size }
    pub fn ty(&self) -> u32 { self.ty }
    pub fn valid(&self) -> bool { self.valid }
    pub fn invalid(&self) -> bool { !self.valid }
    pub fn used(&self) -> bool  { self.used }
    pub fn not_used(&self) -> bool { !self.used }
    pub fn set_ty(&mut self, ty: u32) { self.ty = ty; }
    pub fn set_used(&mut self, used: bool) { self.used = used; }
    pub fn set_valid(&mut self, valid: bool) { self.valid = valid; }
}

#[repr(C)]
pub struct RBPtr { ptr: *const u8 }
impl RBPtr {
    fn new<T>(addr: *const T) -> Self { RBPtr { ptr: addr as *const u8 } }
    pub fn addr(&self) -> usize { self.ptr as usize }
    pub fn ptr<T>(&self) -> *mut T { self.ptr as *mut T }
}

#[repr(C)]
#[derive(Debug)]
pub struct RAMBlockManager {
    blocks: *mut RAMBlock,
    is_init: bool,
    count: usize,
    max: usize
}

pub const BASE_RAMBLOCK_SIZE: usize = 128;
pub static RAM_BLOCKS: [RAMBlock; BASE_RAMBLOCK_SIZE] = [RAMBlock::new_invalid(); BASE_RAMBLOCK_SIZE];
pub const RAM_BLOCKS_INIT_PTR: *const u8 = &raw const RAM_BLOCKS as *const u8;
pub static RAM_BLOCK_MANAGER: Mutex<RAMBlockManager> = Mutex::new(RAMBlockManager {
    blocks: &raw const RAM_BLOCKS as *mut RAMBlock,
    is_init: false, count: 0,
    max: BASE_RAMBLOCK_SIZE,
});
unsafe impl Send for RAMBlock {}
unsafe impl Sync for RAMBlock {}
unsafe impl Send for RAMBlockManager {}
unsafe impl Sync for RAMBlockManager {}

impl RAMBlockManager {
    pub fn init(&mut self) {
        let mut ember = EMBER.lock();
        ember.sort_ram_layout_by(|desc| desc.page_count);
        if self.is_init { return; } self.count = 0;
        for desc in ember.ram_layout().iter().rev() {
            if desc.ty == ramtype::CONVENTIONAL {
                let size = desc.page_count as usize * PAGE_4KIB;
                let addr = desc.phys_start as *const u8;
                self.add(addr, size, desc.ty, false);
            }
        }
        ember.sort_ram_layout_by(|desc| desc.phys_start);
        for desc in ember.ram_layout() {
            if desc.ty != ramtype::CONVENTIONAL {
                let size = desc.page_count as usize * PAGE_4KIB;
                let addr = desc.phys_start as *const u8;
                self.add(addr, size, desc.ty, true);
            }
        }
    }

    pub fn identity_map_size(&self) -> usize {
        return self.blocks().iter()
            .filter(|&block| block.valid())
            .map(|&block| block.addr() + block.size())
            .max().unwrap_or(0);
    }

    pub fn count_filter(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.blocks().iter().filter(|&block| block.valid() && filter(block))
            .map(|block| block.size()).sum();
    }

    pub fn available(&self) -> usize {
        return self.count_filter(|block| block.not_used() && block.ty() == ramtype::CONVENTIONAL);
    }

    pub fn total(&self) -> usize {
        return self.count_filter(|_| true);
    }

    pub fn blocks(&self) -> &[RAMBlock] {
        unsafe { core::slice::from_raw_parts(self.blocks, self.count) }
    }

    fn blocks_mut(&mut self) -> &mut [RAMBlock] {
        unsafe { core::slice::from_raw_parts_mut(self.blocks, self.count) }
    }

    pub fn blocks_iter(&self) -> impl Iterator<Item = &RAMBlock> {
        self.blocks().iter().filter(|&block| block.valid())
    }

    pub fn blocks_iter_mut(&mut self) -> impl Iterator<Item = &mut RAMBlock> {
        self.blocks_mut().iter_mut().filter(|block| block.valid())
    }

    pub fn sort(&mut self) {
        use crate::sort::HeaplessSort;
        self.blocks_mut().sort_noheap_by(|a, b|
            match (a.valid(), b.valid()) {
                (true, true)   => a.addr.cmp(&b.addr),
                (true, false)  => core::cmp::Ordering::Less,
                (false, true)  => core::cmp::Ordering::Greater,
                (false, false) => core::cmp::Ordering::Equal,
            }
        );
    }

    pub fn find_free_ram(&self, size: usize, ty: u32) -> Option<RBPtr> {
        return self.blocks_iter()
            .find(|&block| block.not_used() && block.size() >= size && block.ty() == ty)
            .map(|block| RBPtr::new(block.ptr()));
    }

    fn add(&mut self, addr: *const u8, size: usize, ty: u32, used: bool) {
        let size = align_up(size, PAGE_4KIB);
        if self.count >= self.max { self.expand(self.max * 2); }
        let idx = self.count; self.count += 1;
        let blocks = self.blocks_mut();
        blocks[idx] = RAMBlock::new(addr, size, ty, used);

        for i in (1..=idx).rev() {
            let (current, prev) = (blocks[i], blocks[i - 1]);
            if !current.valid() || !prev.valid() { break; }
            if current.ptr() >= prev.ptr() { break; }
            blocks.swap(i, i - 1);
        }
    }

    pub fn reserve_at_as(&mut self, addr: *const u8, size: usize, ty: u32, as_ty: u32, used: bool) -> Option<RBPtr> {
        let size = align_up(size, PAGE_4KIB);
        let target_idx = self.blocks_iter().position(|block| {
            block.not_used() && block.ty() == ty &&
            addr >= block.ptr() && addr as usize + size <= block.addr() + block.size()
        });

        if let Some(idx) = target_idx {
            let block = self.blocks()[idx];
            let block_end = block.addr() + block.size();
            let before_size = addr as usize - block.addr();
            let after_size = block_end - (addr as usize + size);

            self.blocks_mut()[idx] = RAMBlock::new(addr, size, as_ty, used);
            if before_size > 0 { self.add(block.ptr(), before_size, block.ty(), block.used()); }
            if after_size > 0  { self.add(unsafe { addr.add(size) }, after_size, block.ty(), block.used()); }
            return Some(RBPtr::new(addr));
        }

        return None;
    }

    pub fn reserve_as(&mut self, size: usize, ty: u32, as_ty: u32, used: bool) -> Option<RBPtr> {
        return self.find_free_ram(size, ty)
            .and_then(|ptr| self.reserve_at_as(ptr.ptr(), size, ty, as_ty, used));
    }

    pub fn alloc_at(&mut self, addr: *const u8, size: usize, ty: u32) -> Option<RBPtr> {
        return self.reserve_at_as(addr, size, ty, ty, true);
    }

    pub fn alloc_as(&mut self, size: usize, ty: u32, as_ty: u32) -> Option<RBPtr> {
        return self.reserve_as(size, ty, as_ty, true);
    }

    pub fn alloc(&mut self, size: usize, ty: u32) -> Option<RBPtr> {
        return self.find_free_ram(size, ty)
            .and_then(|ptr| self.alloc_at(ptr.ptr(), size, ty));
    }

    pub fn free(&mut self, ptr: RBPtr) {
        let found = self.blocks_iter_mut()
            .find(|block| block.ptr() <= ptr.ptr() && block.addr() + block.size() > ptr.addr());
        if let Some(block) = found { block.set_used(false); }
    }

    pub unsafe fn free_raw(&mut self, ptr: *const u8) {
        let found = self.blocks_iter_mut()
            .find(|block| block.ptr() as *const u8 <= ptr && block.addr() + block.size() > ptr as usize);
        if let Some(block) = found { block.set_used(false); }
    }

    pub fn expand(&mut self, new_max: usize) {
        if new_max <= self.max { return; }

        let manager_size = new_max * core::mem::size_of::<RAMBlock>();
        let old_blocks_ptr = self.blocks;
        let new_blocks_ptr = self.find_free_ram(manager_size, ramtype::CONVENTIONAL).unwrap().ptr();
        unsafe { core::ptr::copy(old_blocks_ptr, new_blocks_ptr, self.count); }
        (self.blocks, self.max) = (new_blocks_ptr, new_max);
        self.alloc_at(new_blocks_ptr as *mut u8, manager_size, ramtype::CONVENTIONAL);
        if old_blocks_ptr as *const u8 == RAM_BLOCKS_INIT_PTR { return; }
        self.free(RBPtr::new(old_blocks_ptr));
    }
}