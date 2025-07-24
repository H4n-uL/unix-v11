use crate::{
    ram::{align_up, PAGE_4KIB},
    sort::HeaplessSort,
    sysinfo::{ramtype, RAMDescriptor, NON_RAM}
};
use spin::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlock {
    ptr: *const u8,
    size: usize,
    ty: u32,
    used: bool
}

impl RAMBlock {
    pub fn new(ptr: *const u8, size: usize, ty: u32, used: bool) -> Self {
        return Self { ptr, size, ty, used };
    }
    pub const fn new_invalid() -> Self {
        return Self { ptr: 0 as *const u8, size: 0, ty: 0, used: false };
    }

    pub fn addr(&self) -> usize    { self.ptr as usize }
    pub fn ptr(&self) -> *mut u8   { self.ptr as *mut u8 }
    pub fn size(&self) -> usize    { self.size }
    pub fn ty(&self) -> u32        { self.ty }
    pub fn valid(&self) -> bool    { self.size > 0 }
    pub fn invalid(&self) -> bool  { self.size == 0 }
    pub fn used(&self) -> bool     { self.used }
    pub fn not_used(&self) -> bool { !self.used }

    fn set_ptr(&mut self, ptr: *const u8) { self.ptr  = ptr; }
    fn set_size(&mut self, size: usize)   { self.size = size; }
    fn set_ty(&mut self, ty: u32)         { self.ty   = ty; }
    fn set_used(&mut self, used: bool)    { self.used = used; }
    fn invalidate(&mut self)              { self.size = 0; }

    fn is_coalescable(&self, other: &RAMBlock) -> i8 {
        let info_eq = {
            self.valid() && other.valid() &&
            self.ty == other.ty &&
            self.used == other.used
        };
        if !info_eq { return 0; }
        return
             if self.addr() + self.size == other.addr() { -1 } // self is before other
        else if other.addr() + other.size == self.addr() { 1 } // self is after other
        else { 0 };
    }

    pub fn into_owned_ptr(&self) -> OwnedPtr {
        return OwnedPtr::new_bytes(self.ptr, self.size);
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct OwnedPtr {
    ptr: *const u8,
    size: usize
}

impl OwnedPtr {
    const fn new_bytes<T>(ptr: *const T, size: usize) -> Self {
        Self { ptr: ptr as *const u8, size }
    }

    const fn new_typed<T>(ptr: *const T, count: usize) -> Self {
        Self::new_bytes(ptr, count * size_of::<T>())
    }

    const fn from_slice<T>(slice: &[T]) -> Self {
        Self::new_typed(slice.as_ptr(), slice.len())
    }

    pub fn addr(&self) -> usize { self.ptr as usize }
    pub fn ptr<T>(&self) -> *mut T { self.ptr as *mut T }
    pub fn size(&self) -> usize { self.size }
    pub unsafe fn clone(&self) -> Self { Self::new_bytes(self.ptr, self.size) }

    pub fn merge(&self, other: &Self) -> Option<Self> {
        if self.addr() + self.size != other.addr() { return None; } // Not adjacent
        return Some(Self::new_bytes(self.ptr, self.size + other.size));
    }

    pub fn split(&self, offset: usize) -> Option<(Self, Self)> {
        if offset >= self.size { return None; } // Offset out of bounds
        let first = Self::new_bytes(self.ptr, offset);
        let second = Self::new_bytes((self.addr() + offset) as *const u8, self.size - offset);
        return Some((first, second));
    }
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
        self.addr = self.addr.map(|a| align_up(a as _, self.align) as _);
        self
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct PhysAllocData {
    ptr: OwnedPtr,
    max: usize,
    is_init: bool
}

pub struct PhysAlloc(Mutex<PhysAllocData>);

const BASE_RB_SIZE: usize = 128;
static RB_EMBEDDED: [RAMBlock; BASE_RB_SIZE] = [RAMBlock::new_invalid(); BASE_RB_SIZE];
pub static PHYS_ALLOC: PhysAlloc = PhysAlloc::empty(&RB_EMBEDDED);

unsafe impl Send for RAMBlock {}
unsafe impl Sync for RAMBlock {}
unsafe impl Send for PhysAllocData {}
unsafe impl Sync for PhysAllocData {}

impl PhysAllocData {
    const fn empty(rb: &[RAMBlock]) -> Self {
        Self {
            ptr: OwnedPtr::from_slice(rb),
            is_init: false,
            max: rb.len()
        }
    }

    fn init(&mut self, efi_ram_layout: &mut [RAMDescriptor]) {
        if self.is_init { return; }
        efi_ram_layout.sort_noheap_by_key(|desc| desc.page_count);
        for desc in efi_ram_layout.iter().rev() {
            if desc.ty == ramtype::CONVENTIONAL {
                let size = desc.page_count as usize * PAGE_4KIB;
                let ptr = desc.phys_start as *const u8;
                self.add(ptr, size, desc.ty, false);
            }
        }
        efi_ram_layout.sort_noheap_by_key(|desc| desc.phys_start);
        for desc in efi_ram_layout {
            if desc.ty != ramtype::CONVENTIONAL {
                let size = desc.page_count as usize * PAGE_4KIB;
                let ptr = desc.phys_start as *const u8;
                self.add(ptr, size, desc.ty, true);
            }
        }
        self.is_init = true;
    }

    fn blocks_raw(&self) -> &[RAMBlock] {
        return unsafe { core::slice::from_raw_parts(self.ptr.ptr(), self.max) };
    }

    fn blocks_raw_mut(&mut self) -> &mut [RAMBlock] {
        return unsafe { core::slice::from_raw_parts_mut(self.ptr.ptr() as *mut RAMBlock, self.max) };
    }

    fn blocks_iter(&self) -> impl Iterator<Item = &RAMBlock> {
        return self.blocks_raw().iter().filter(|&block| block.valid());
    }

    fn blocks_iter_mut(&mut self) -> impl Iterator<Item = &mut RAMBlock> {
        return self.blocks_raw_mut().iter_mut().filter(|block| block.valid());
    }

    fn count(&self) -> usize { return self.blocks_iter().count(); }

    fn size_filter(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.blocks_iter().filter(|&block| filter(block))
            .map(|block| block.size()).sum();
    }

    fn sort(&mut self) {
        self.blocks_raw_mut().sort_noheap_by(|a, b|
            match (a.valid(), b.valid()) {
                ( true,  true) => a.addr().cmp(&b.addr()),
                ( true, false) => core::cmp::Ordering::Less,
                (false,  true) => core::cmp::Ordering::Greater,
                (false, false) => core::cmp::Ordering::Equal
            }
        );
    }

    fn find(&mut self, mut f: impl FnMut(&RAMBlock) -> bool) -> Option<&mut RAMBlock> {
        return self.blocks_iter_mut().find(|block| f(block));
    }

    fn find_free_ram(&mut self, args: AllocParams) -> Option<OwnedPtr> {
        let args = args.aligned();
        return self.find(|block|
            block.not_used() && block.size() >= args.size && block.ty() == args.from_type
        ).map(|block| OwnedPtr::new_bytes(block.ptr(), args.size));
    }

    fn alloc(&mut self, args: AllocParams) -> Option<OwnedPtr> {
        let args = args.aligned();
        if NON_RAM.contains(&args.from_type) || NON_RAM.contains(&args.as_type) {
            return None; // Cannot allocate from or as non-RAM types
        }
        let ptr = match args.addr {
            Some(addr) => addr,
            None => self.find_free_ram(args)?.ptr()
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
            let after_ptr = (ptr as usize + args.size) as *const u8;
            let after = block.addr() + block.size() - after_ptr as usize;
            if before > 0 { self.add(block.ptr(), before, block.ty(), false); }
            if after > 0 { self.add(after_ptr, after, block.ty(), false); }
            return Some(OwnedPtr::new_bytes(ptr, args.size));
        }

        return None;
    }

    fn free(&mut self, ptr: OwnedPtr) {
        let found_block = self.find(|block|
            block.ptr() <= ptr.ptr() && block.addr() + block.size() > ptr.addr()
        );

        if let Some(block) = found_block {
            let block_cp = *block;
            let free_start = ptr.addr();
            let free_end = (ptr.addr() + ptr.size()).min(block_cp.addr() + block_cp.size());
            let free_size = free_end - free_start;

            block.invalidate();
            if block_cp.addr() < free_start {
                let before_size = free_start - block_cp.addr();
                self.add(block_cp.ptr(), before_size, block_cp.ty(), block_cp.used());
            }
            self.add(free_start as *const u8, free_size, ramtype::CONVENTIONAL, false);
            if free_end < block_cp.addr() + block_cp.size() {
                let after_start = free_end as *const u8;
                let after_size = block_cp.addr() + block_cp.size() - free_end;
                self.add(after_start, after_size, block_cp.ty(), block_cp.used());
            }
        }
    }

    fn add(&mut self, ptr: *const u8, size: usize, ty: u32, used: bool) {
        let new_block = RAMBlock::new(ptr, size, ty, used);
        let (mut before, mut after) = (None, None);
        for block in self.blocks_iter_mut() {
            match new_block.is_coalescable(&block) {
                -1 => { after = Some(block); },
                1 => { before = Some(block); },
                _ => continue
            }
        }

        match (before, after) {
            (Some(before_block), Some(after_block)) => {
                before_block.set_size(before_block.size() + new_block.size() + after_block.size());
                after_block.invalidate();
            },
            (Some(before_block), None) => {
                before_block.set_size(before_block.size() + new_block.size());
            },
            (None, Some(after_block)) => {
                after_block.set_ptr(new_block.ptr());
                after_block.set_size(after_block.size() + new_block.size());
            },
            (None, None) => {
                if self.count() >= self.max { self.expand(self.max * 2); }
                let blocks = self.blocks_raw_mut();
                let mut idx = 0;
                for block in &mut *blocks {
                    if block.valid() { idx += 1; continue; }
                    *block = new_block;
                    break;
                }

                for i in (1..=idx).rev() {
                    let (current, prev) = (blocks[i], blocks[i - 1]);
                    if current.ptr() >= prev.ptr() { break; }
                    blocks.swap(i, i - 1);
                }
            }
        }
    }

    fn expand(&mut self, new_max: usize) {
        if new_max <= self.max { return; }

        let alloc_param = AllocParams::new(new_max * size_of::<RAMBlock>());

        let old_blocks = unsafe { self.ptr.clone() };
        let new_blocks = self.find_free_ram(alloc_param).unwrap();
        let old_ptr = old_blocks.ptr::<RAMBlock>();
        let new_ptr = new_blocks.ptr::<RAMBlock>();
        unsafe {
            core::ptr::write_bytes(new_ptr, 0, new_max);
            core::ptr::copy(old_ptr, new_ptr, self.max);
        }
        (self.ptr, self.max) = (new_blocks, new_max);
        if old_blocks.ptr() as *const RAMBlock != RB_EMBEDDED.as_ptr() {
            self.free(old_blocks);
        }
        self.alloc(alloc_param.at(new_ptr));
    }
}

impl PhysAlloc {
    const fn empty(rb: &[RAMBlock]) -> Self {
        return Self(Mutex::new(PhysAllocData::empty(rb)));
    }

    pub fn init(&self, efi_ram_layout: &mut [RAMDescriptor]) { self.0.lock().init(efi_ram_layout); }

    pub fn available(&self) -> usize {
        return self.0.lock().size_filter(|block| block.not_used() && block.ty() == ramtype::CONVENTIONAL);
    }

    pub fn total(&self) -> usize {
        return self.0.lock().size_filter(|block| !NON_RAM.contains(&block.ty()));
    }

    pub fn sort(&self) { self.0.lock().sort(); }

    pub fn with_blocks<F, R>(&self, f: F) -> R
    where F: FnOnce(&dyn Iterator<Item = &RAMBlock>) -> R {
        f(&self.0.lock().blocks_iter())
    }

    pub fn find_free_ram(&self, args: AllocParams) -> Option<OwnedPtr> {
        return self.0.lock().find_free_ram(args);
    }

    pub fn alloc(&self, args: AllocParams) -> Option<OwnedPtr> {
        return self.0.lock().alloc(args);
    }

    pub fn free(&self, ptr: OwnedPtr) {
        self.0.lock().free(ptr);
    }

    pub unsafe fn free_raw(&self, ptr: *mut u8, size: usize) {
        self.free(OwnedPtr::new_bytes(ptr, size));
    }

    pub fn expand(&self, new_max: usize) {
        self.0.lock().expand(new_max);
    }
}