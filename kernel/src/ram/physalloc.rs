use crate::{
    ram::{PAGE_4KIB, align_up},
    sort::HeaplessSort,
    sysinfo::{NON_RAM, RAMDescriptor, RAMType}
};

use core::cmp::Ordering;
use spin::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlock {
    ptr: usize,
    size: usize,
    ty: RAMType,
    used: bool
}

impl RAMBlock {
    pub fn new(ptr: *const u8, size: usize, ty: RAMType, used: bool) -> Self {
        return Self { ptr: ptr as usize, size, ty, used };
    }
    pub const fn new_invalid() -> Self {
        return Self { ptr: 0, size: 0, ty: RAMType::Reserved, used: false };
    }

    pub fn addr(&self) -> usize    { self.ptr }
    pub fn ptr(&self) -> *mut u8   { self.ptr as *mut u8 }
    pub fn size(&self) -> usize    { self.size }
    pub fn ty(&self) -> RAMType    { self.ty }
    pub fn valid(&self) -> bool    { self.size > 0 }
    pub fn invalid(&self) -> bool  { self.size == 0 }
    pub fn used(&self) -> bool     { self.used }
    pub fn not_used(&self) -> bool { !self.used }

    fn set_ptr(&mut self, ptr: *const u8) { self.ptr  = ptr as usize; }
    fn set_size(&mut self, size: usize)   { self.size = size; }
    fn set_ty(&mut self, ty: RAMType)     { self.ty   = ty; }
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
        return OwnedPtr::new_bytes(self.addr(), self.size);
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct OwnedPtr {
    ptr: usize,
    size: usize
}

impl OwnedPtr {
    const fn new_bytes(ptr: usize, size: usize) -> Self {
        Self { ptr, size }
    }

    const fn null() -> Self {
        Self { ptr: 0, size: 0 }
    }

    const fn new_typed<T>(ptr: usize, count: usize) -> Self {
        Self::new_bytes(ptr, count * size_of::<T>())
    }

    fn from_slice<'a, T>(slice: &'a [T]) -> Self {
        Self::new_typed::<T>(slice.as_ptr() as usize, slice.len())
    }

    pub fn into_slice<T>(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.ptr::<T>(), self.size / size_of::<T>()) }
    }

    pub fn into_slice_mut<T>(&self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr::<T>(), self.size / size_of::<T>()) }
    }

    pub fn addr(&self) -> usize { self.ptr as usize }
    pub fn ptr<T>(&self) -> *mut T { self.ptr as *mut T }
    pub fn size(&self) -> usize { self.size }
    pub unsafe fn clone(&self) -> Self { Self::new_bytes(self.addr(), self.size()) }

    pub fn merge(&mut self, other: Self) -> Result<(), Self> {
        if self.addr() + self.size() == other.addr() {
            self.size += other.size();
            return Ok(());
        } else if other.addr() + other.size() == self.addr() {
            self.ptr = other.ptr;
            self.size += other.size();
            return Ok(());
        }
        return Err(other);
    }

    pub fn split(&mut self, offset: usize) -> Result<Self, ()> {
        if offset >= self.size { return Err(()); } // Offset out of bounds
        let other = Self::new_bytes(self.addr() + offset, self.size - offset);
        self.size = offset;
        return Ok(other);
    }
}

#[derive(Clone, Copy)]
pub struct AllocParams {
    addr: Option<*const u8>,
    size: usize,
    align: usize,
    from_type: RAMType,
    as_type: RAMType,
    used: bool
}

impl AllocParams {
    pub fn new(size: usize) -> Self {
        Self {
            addr: None, size, align: PAGE_4KIB,
            from_type: RAMType::Conv,
            as_type: RAMType::Conv,
            used: true
        }
    }

    pub fn at<T>(mut self, addr: *mut T) -> Self { self.addr = Some(addr as *const u8); self }
    pub fn align(mut self, align: usize) -> Self { self.align = align.max(1); self }
    pub fn from_type(mut self, ty: RAMType) -> Self { self.from_type = ty; self }
    pub fn as_type(mut self, ty: RAMType) -> Self { self.as_type = ty; self }
    pub fn reserve(mut self) -> Self { self.used = false; self }

    pub fn build(self) -> Self {
        Self {
            addr: self.addr.map(|a| align_up(a as _, self.align) as _),
            size: align_up(self.size, self.align),
            align: self.align,
            from_type: self.from_type,
            as_type: self.as_type,
            used: self.used
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct PhysAlloc {
    ptr: OwnedPtr,
    max: usize,
    is_init: bool
}

pub struct PhysAllocGlob(pub Mutex<PhysAlloc>);

const BASE_RB_SIZE: usize = 128;
static RB_EMBEDDED: [RAMBlock; BASE_RB_SIZE] = [RAMBlock::new_invalid(); BASE_RB_SIZE];
pub static PHYS_ALLOC: PhysAllocGlob = PhysAllocGlob::empty();

unsafe impl Send for RAMBlock {}
unsafe impl Sync for RAMBlock {}
unsafe impl Send for PhysAlloc {}
unsafe impl Sync for PhysAlloc {}

impl PhysAlloc {
    const fn empty() -> Self {
        Self {
            ptr: OwnedPtr::null(),
            is_init: false,
            max: 0
        }
    }

    fn init(&mut self, efi_ram_layout: &mut [RAMDescriptor]) {
        let rb = &RB_EMBEDDED;
        if self.is_init { return; }
        (self.ptr, self.max) = (OwnedPtr::from_slice(rb), rb.len());
        efi_ram_layout.sort_noheap_by_key(|desc| desc.page_count);
        for desc in efi_ram_layout.iter().rev() {
            if desc.ty == RAMType::Conv {
                let size = desc.page_count as usize * PAGE_4KIB;
                let ptr = desc.phys_start as *const u8;
                let block = RAMBlock::new(ptr, size, desc.ty, false);
                self.add(block, false);
            }
        }

        let new_rb = self.alloc(
            AllocParams::new(size_of::<RAMBlock>() * self.max)
        ).expect("Failed to relocate RAMBlocks");
        unsafe { self.ptr.ptr::<RAMBlock>().copy_to(new_rb.ptr::<RAMBlock>(), self.max); }
        self.ptr = new_rb;

        efi_ram_layout.sort_noheap_by_key(|desc| desc.phys_start);
        for desc in efi_ram_layout {
            if desc.ty != RAMType::Conv {
                let size = desc.page_count as usize * PAGE_4KIB;
                let ptr = desc.phys_start as *const u8;
                let block = RAMBlock::new(ptr, size, desc.ty, false);
                self.add(block, false);
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
                ( true, false) => Ordering::Less,
                (false,  true) => Ordering::Greater,
                (false, false) => Ordering::Equal
            }
        );
    }

    fn find(&mut self, mut f: impl FnMut(&RAMBlock) -> bool) -> Option<&mut RAMBlock> {
        return self.blocks_iter_mut().find(|block| f(block));
    }

    fn find_free_ram(&mut self, args: AllocParams) -> Option<OwnedPtr> {
        let args = args.build();
        return self.find(|block|
            block.not_used() && block.size() >= args.size && block.ty() == args.from_type
        ).map(|block| OwnedPtr::new_bytes(block.addr(), args.size));
    }

    fn alloc(&mut self, args: AllocParams) -> Option<OwnedPtr> {
        let args = args.build();
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

        struct AllocInfo {
            from: RAMBlock,
            to: RAMBlock
        }
        let mut alloc_info = None;
        for block in self.blocks_iter_mut() {
            if filter(block) {
                if block.ty() == args.as_type && !args.used { break; }
                alloc_info = Some(AllocInfo {
                    from: *block,
                    to: RAMBlock::new(ptr, args.size, args.as_type, args.used)
                });
                block.invalidate();
                break;
            }
        }

        if let Some(ainfo) = alloc_info {
            self.add(ainfo.to, true);

            let before_block = RAMBlock::new(
                ainfo.from.ptr(), ptr as usize - ainfo.from.addr(),
                ainfo.from.ty(), false
            );
            let after_block = RAMBlock::new(
                (ptr as usize + args.size) as *const u8,
                ainfo.from.addr() + ainfo.from.size() - (ptr as usize + args.size),
                ainfo.from.ty(), false
            );
            self.add(before_block, false);
            self.add(after_block, false);

            return Some(ainfo.to.into_owned_ptr());
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
                let before_block = RAMBlock::new(block_cp.ptr(), before_size, block_cp.ty(), block_cp.used());
                self.add(before_block, false);
            }
            let this_block = RAMBlock::new(free_start as *const u8, free_size, RAMType::Conv, false);
            self.add(this_block, false);
            if free_end < block_cp.addr() + block_cp.size() {
                let after_start = free_end as *const u8;
                let after_size = block_cp.addr() + block_cp.size() - free_end;
                let after_block = RAMBlock::new(after_start, after_size, block_cp.ty(), block_cp.used());
                self.add(after_block, false);
            }
        }
    }

    fn add(&mut self, new_block: RAMBlock, alloc: bool) {
        if new_block.invalid() { return; }
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
                let prereq = if alloc { Some(new_block.into_owned_ptr()) } else { None };
                if self.count() >= self.max { self.expand(self.max * 2, prereq); }
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

    fn expand(&mut self, new_max: usize, prereq: Option<OwnedPtr>) {
        if new_max <= self.max { return; }

        let alloc_param = AllocParams::new(new_max * size_of::<RAMBlock>());
        let old_blocks = unsafe { self.ptr.clone() };

        let new_blocks = self.find(|block| {
            return {
                block.size() >= alloc_param.size && !block.used()
                && block.ty() == alloc_param.from_type
                && prereq.as_ref().map_or(true, |p| {
                    let noteqblk = {
                        block.addr() + block.size() <= p.addr()
                        || block.addr() >= p.addr() + p.size()
                    };
                    let before = p.addr().saturating_sub(block.addr());
                    let after = (block.addr() + block.size()).saturating_sub(p.addr() + p.size());
                    return noteqblk || before >= alloc_param.size || after >= alloc_param.size;
                })
            };
        }).map(|block| {
            let addr = prereq.as_ref().map_or(block.addr(), |p| {
                if {
                    block.addr() + block.size() <= p.addr()
                    || block.addr() >= p.addr() + p.size()
                    || p.addr().saturating_sub(block.addr()) >= alloc_param.size
                } { return block.addr(); }
                return p.addr() + p.size();
            });
            OwnedPtr::new_bytes(addr, alloc_param.size)
        }).expect("Failed to expand RAMBlocks");

        unsafe {
            new_blocks.ptr::<RAMBlock>().write_bytes(0, new_max);
            old_blocks.ptr::<RAMBlock>().copy_to(new_blocks.ptr::<RAMBlock>(), self.max);
        }
        (self.ptr, self.max) = (new_blocks, new_max);
        self.free(old_blocks);
        self.alloc(alloc_param.at(self.ptr.ptr::<RAMBlock>()));
    }
}

impl PhysAllocGlob {
    const fn empty() -> Self {
        return Self(Mutex::new(PhysAlloc::empty()));
    }

    pub fn init(&self, efi_ram_layout: &mut [RAMDescriptor]) { self.0.lock().init(efi_ram_layout); }

    pub fn available(&self) -> usize {
        return self.0.lock().size_filter(|block| block.not_used() && block.ty() == RAMType::Conv);
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
        self.free(OwnedPtr::new_bytes(ptr as usize, size));
    }

    pub fn expand(&self, new_max: usize) {
        self.0.lock().expand(new_max, None);
    }
}
