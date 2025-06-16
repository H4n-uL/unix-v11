#![allow(dead_code)]
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
        return Self { addr, size, ty, valid: true, used };
    }
    pub const fn new_invalid() -> Self {
        return Self { addr: 0 as *const u8, size: 0, ty: 0, valid: false, used: false };
    }

    pub fn addr(&self) -> usize    {  self.addr as usize }
    pub fn ptr(&self) -> *mut u8   {  self.addr as *mut u8 }
    pub fn size(&self) -> usize    {  self.size }
    pub fn ty(&self) -> u32        {  self.ty }
    pub fn valid(&self) -> bool    {  self.valid }
    pub fn invalid(&self) -> bool  { !self.valid }
    pub fn used(&self) -> bool     {  self.used }
    pub fn not_used(&self) -> bool { !self.used }
    fn set_ty(&mut self, ty: u32)        { self.ty    = ty; }
    fn set_used(&mut self, used: bool)   { self.used  = used; }
    fn set_valid(&mut self, valid: bool) { self.valid = valid; }
}

#[repr(C)]
pub struct RBPtr {
    ptr: *const u8,
    size: usize
}

impl RBPtr {
    fn new<T>(addr: *const T, count: usize) -> Self {
        RBPtr { ptr: addr as *const u8, size: count * size_of::<T>() }
    }
    pub fn addr(&self) -> usize { self.ptr as usize }
    pub fn ptr<T>(&self) -> *mut T { self.ptr as *mut T }
    pub fn size(&self) -> usize { self.size }
}

#[derive(Clone, Copy)]
pub struct AllocParams {
    addr: Option<*const u8>,
    size: usize,
    align: usize,
    from_type: u32,
    as_type: u32,
    used: bool
}

impl AllocParams {
    pub fn new(size: usize) -> Self {
        Self {
            addr: None, size, align: PAGE_4KIB,
            from_type: ramtype::CONVENTIONAL,
            as_type: ramtype::CONVENTIONAL,
            used: true
        }
    }

    pub fn at<T>(mut self, addr: *mut T) -> Self { self.addr = Some(addr as *const u8); self }
    pub fn align(mut self, align: usize) -> Self { self.align = align.max(1); self }
    pub fn from_type(mut self, ty: u32) -> Self { self.from_type = ty; self }
    pub fn as_type(mut self, ty: u32) -> Self { self.as_type = ty; self }
    pub fn reserve(mut self) -> Self { self.used = false; self }

    fn aligned(mut self) -> Self {
        self.size = align_up(self.size, self.align);
        if let Some(addr) = self.addr {
            self.addr = Some(align_up(addr as usize, self.align) as *const u8);
        }
        self
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct RAMBlockManager {
    blocks: *const RAMBlock,
    is_init: bool,
    count: usize,
    max: usize
}

const BASE_RAMBLOCK_SIZE: usize = 128;
static RAMBLOCKS_EMBEDDED: [RAMBlock; BASE_RAMBLOCK_SIZE] = [RAMBlock::new_invalid(); BASE_RAMBLOCK_SIZE];
static RAMBLOCK_MANAGER: Mutex<RAMBlockManager> = Mutex::new(RAMBlockManager::empty(&RAMBLOCKS_EMBEDDED));

unsafe impl Send for RAMBlock {}
unsafe impl Sync for RAMBlock {}
unsafe impl Send for RAMBlockManager {}
unsafe impl Sync for RAMBlockManager {}

impl RAMBlockManager {
    const fn empty(rb: &[RAMBlock]) -> Self {
        RAMBlockManager { blocks: rb.as_ptr(), is_init: false, count: 0, max: rb.len() }
    }

    fn init(&mut self) {
        let mut ember = EMBER.lock();
        if self.is_init { return; } self.count = 0;
        ember.sort_ram_layout_by(|desc| desc.page_count);
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

    fn blocks_raw(&self) -> &[RAMBlock] {
        return unsafe { core::slice::from_raw_parts(self.blocks, self.max) };
    }

    fn blocks_raw_mut(&mut self) -> &mut [RAMBlock] {
        return unsafe { core::slice::from_raw_parts_mut(self.blocks as *mut RAMBlock, self.max) };
    }

    fn blocks_iter(&self) -> impl Iterator<Item = &RAMBlock> {
        return self.blocks_raw().iter().filter(|&block| block.valid());
    }

    fn blocks_iter_mut(&mut self) -> impl Iterator<Item = &mut RAMBlock> {
        return self.blocks_raw_mut().iter_mut().filter(|block| block.valid());
    }

    fn count_filter(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.blocks_iter().filter(|&block| filter(block))
            .map(|block| block.size()).sum();
    }

    fn available(&self) -> usize {
        return self.count_filter(|block| block.not_used() && block.ty() == ramtype::CONVENTIONAL);
    }

    fn total(&self) -> usize {
        return self.count_filter(|block| block.ty() == ramtype::CONVENTIONAL);
    }

    fn sort(&mut self) {
        use crate::sort::HeaplessSort;
        self.blocks_raw_mut().sort_noheap_by(|a, b|
            match (a.valid(), b.valid()) {
                (true, true)   => a.addr.cmp(&b.addr),
                (true, false)  => core::cmp::Ordering::Less,
                (false, true)  => core::cmp::Ordering::Greater,
                (false, false) => core::cmp::Ordering::Equal,
            }
        );
    }

    fn find_free_ram(&self, args: AllocParams) -> Option<RBPtr> {
        let args = args.aligned();
        return self.blocks_iter()
            .find(|&block|
                block.not_used() &&
                block.size() >= args.size && block.ty() == args.from_type
            ).map(|block| RBPtr::new(block.ptr(), args.size));
    }

    fn alloc(&mut self, args: AllocParams) -> Option<RBPtr> {
        let args = args.aligned();
        let ptr = match args.addr {
            Some(addr) => addr,
            None => self.find_free_ram(args)?.ptr(),
        };

        let filter = |block: &RAMBlock| {
            block.not_used() && args.from_type == block.ty() &&
            ptr >= block.ptr() && ptr as usize + args.size <= block.addr() + block.size()
        };

        let mut split_info = None;
        for block in self.blocks_iter_mut() {
            if filter(block) {
                if block.ty() == args.as_type && !args.used { break; }
                split_info = Some(*block);
                *block = RAMBlock::new(ptr, args.size, args.as_type, args.used);
                break;
            }
        }

        if let Some(block) = split_info {
            let before = ptr as usize - block.addr();
            let after = block.addr() + block.size() - (ptr as usize + args.size);
            if before > 0 { self.add(block.ptr(), before, block.ty(), false); }
            if after > 0 { self.add(unsafe { ptr.add(args.size) }, after, block.ty(), false); }
            return Some(RBPtr::new(ptr, args.size));
        }

        return None;
    }

    fn free(&mut self, ptr: RBPtr) {
        let found = self.blocks_iter_mut()
            .find(|block| block.ptr() <= ptr.ptr() && block.addr() + block.size() > ptr.addr());
        if let Some(block) = found { block.set_used(false); }
    }

    unsafe fn free_raw(&mut self, ptr: *const u8) {
        let found = self.blocks_iter_mut()
            .find(|block| block.ptr() as *const u8 <= ptr && block.addr() + block.size() > ptr as usize);
        if let Some(block) = found { block.set_used(false); }
    }

    fn add(&mut self, addr: *const u8, size: usize, ty: u32, used: bool) {
        if self.count >= self.max { self.expand(self.max * 2); }
        let idx = self.count; self.count += 1;
        let blocks = self.blocks_raw_mut();
        blocks[idx] = RAMBlock::new(addr, size, ty, used);

        for i in (1..=idx).rev() {
            let (current, prev) = (blocks[i], blocks[i - 1]);
            if current.invalid() || prev.invalid() { break; }
            if current.ptr() >= prev.ptr() { break; }
            blocks.swap(i, i - 1);
        }
    }

    fn expand(&mut self, new_max: usize) {
        if new_max <= self.max { return; }

        let alloc_param =  AllocParams::new(new_max * size_of::<RAMBlock>());
        let old_blocks_ptr = self.blocks;
        let new_blocks_ptr = self.find_free_ram(alloc_param).unwrap().ptr();
        unsafe { core::ptr::copy(old_blocks_ptr, new_blocks_ptr, self.count); }
        (self.blocks, self.max) = (new_blocks_ptr, new_max);
        if old_blocks_ptr != RAMBLOCKS_EMBEDDED.as_ptr() {
            self.free(RBPtr::new(old_blocks_ptr, self.max));
        }
        self.alloc(alloc_param.at(new_blocks_ptr));
    }
}

// Atomic API to RAMBlock Manager
pub fn init() { RAMBLOCK_MANAGER.lock().init() }
pub fn available() -> usize { RAMBLOCK_MANAGER.lock().available() }
pub fn total() -> usize { RAMBLOCK_MANAGER.lock().total() }
pub fn sort() { RAMBLOCK_MANAGER.lock().sort(); }
pub fn find_free_ram(args: AllocParams) -> Option<RBPtr> { RAMBLOCK_MANAGER.lock().find_free_ram(args) }
pub fn alloc(args: AllocParams) -> Option<RBPtr> { RAMBLOCK_MANAGER.lock().alloc(args) }
pub fn free(ptr: RBPtr) { RAMBLOCK_MANAGER.lock().free(ptr) }
pub unsafe fn free_raw(ptr: *const u8) { unsafe { RAMBLOCK_MANAGER.lock().free_raw(ptr) } }
pub fn expand(new_max: usize) { RAMBLOCK_MANAGER.lock().expand(new_max); }