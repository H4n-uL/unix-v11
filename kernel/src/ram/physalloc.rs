use crate::{
    kargs::{
        NON_RAM, RECLAMABLE, KINFO, SYSINFO,
        RAMDescriptor, RAMType, Segment,
        efi_ram_layout, efi_ram_layout_mut, elf_segments
    },
    ram::{
        PAGE_4KIB, align_up, glacier::page_size, mutex::IntLock, size_align
    },
    sort::HeaplessSort
};

// use core::cmp::Ordering;
use spin::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlock {
    addr: usize,
    size: usize,
    ty: RAMType,
    used: bool
}

impl RAMBlock {
    pub fn new(addr: usize, size: usize, ty: RAMType, used: bool) -> Self {
        return Self { addr, size, ty, used };
    }
    pub const fn new_invalid() -> Self {
        return Self { addr: 0, size: 0, ty: RAMType::Reserved, used: false };
    }

    pub fn addr(&self) -> usize    { self.addr }
    pub fn size(&self) -> usize    { self.size }
    pub fn ty(&self) -> RAMType    { self.ty }
    pub fn valid(&self) -> bool    { self.size > 0 }
    pub fn invalid(&self) -> bool  { self.size == 0 }
    pub fn used(&self) -> bool     { self.used }
    pub fn not_used(&self) -> bool { !self.used }

    fn set_addr(&mut self, addr: usize) { self.addr  = addr; }
    fn set_size(&mut self, size: usize) { self.size = size; }
    fn set_ty(&mut self, ty: RAMType)   { self.ty   = ty; }
    fn set_used(&mut self, used: bool)  { self.used = used; }
    fn invalidate(&mut self)            { self.size = 0; }

    fn is_mergable(&self, other: &RAMBlock) -> i8 {
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
#[derive(Debug, PartialEq, Eq)]
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
    addr: Option<usize>,
    size: usize,
    align: usize,
    from_type: RAMType,
    as_type: RAMType,
    used: bool
}

impl AllocParams {
    pub fn new(size: usize) -> Self {
        return Self {
            addr: None, size,
            align: page_size(),
            from_type: RAMType::Conv,
            as_type: RAMType::Conv,
            used: true
        };
    }

    pub fn at<T>(mut self, addr: *mut T) -> Self { self.addr = Some(addr as usize); self }
    pub fn align(mut self, align: usize) -> Self { self.align = align.max(1); self }
    pub fn from_type(mut self, ty: RAMType) -> Self { self.from_type = ty; self }
    pub fn as_type(mut self, ty: RAMType) -> Self { self.as_type = ty; self }
    pub fn reserve(mut self) -> Self { self.used = false; self }

    pub fn build(mut self) -> Self {
        self.addr = self.addr.map(|a| align_up(a, self.align));
        self.size = size_align(self.size);
        return self;
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct PhysAlloc {
    ptr: OwnedPtr,
    max: usize,
    is_init: bool
}

pub struct PhysAllocGlob(pub IntLock<Mutex<()>, PhysAlloc>);

const BASE_RB_SIZE: usize = 128;
const MIN_REQ: usize = 4;

static mut RB_EMBEDDED: [RAMBlock; BASE_RB_SIZE] = [RAMBlock::new_invalid(); BASE_RB_SIZE];
pub static PHYS_ALLOC: PhysAllocGlob = PhysAllocGlob::empty();

impl PhysAlloc {
    const fn empty() -> Self {
        Self {
            ptr: OwnedPtr::null(),
            is_init: false,
            max: 0
        }
    }

    fn init(&mut self) {
        {
            let efi_ram = efi_ram_layout_mut();

            let rb = unsafe {
                let rb = &raw const RB_EMBEDDED;
                core::slice::from_raw_parts_mut(rb as *mut RAMBlock, (*rb).len())
            };
            if self.is_init { return; }
            (self.ptr, self.max) = (OwnedPtr::from_slice(rb), rb.len());
            efi_ram.sort_noheap_by_key(|desc| desc.page_count);
            for desc in efi_ram.iter().rev() {
                if desc.ty == RAMType::Conv {
                    let size = desc.page_count as usize * PAGE_4KIB;
                    let addr = desc.phys_start as usize;

                    let ty = cfg!(target_arch = "x86_64").then(|| {
                        if addr < 0x100000 { RAMType::Reserved } else { desc.ty }
                    }).unwrap_or(desc.ty);

                    let block = RAMBlock::new(addr, size, ty, false);
                    self.add(block);
                }
            }

            if self.ptr == OwnedPtr::from_slice(rb) {
                let new_rb = self.alloc(
                    AllocParams::new(size_of::<RAMBlock>() * self.max)
                ).expect("Failed to relocate RAMBlocks");
                unsafe { self.ptr.ptr::<RAMBlock>().copy_to(new_rb.ptr::<RAMBlock>(), self.max); }
                self.ptr = new_rb;
            }
        }

        let efi_ram = efi_ram_layout();
        let efi_ptr = self.alloc(
            AllocParams::new(efi_ram.len() * size_of::<RAMDescriptor>())
                .as_type(RAMType::EfiRamLayout)
        ).unwrap();

        let elf_seg = elf_segments();
        let elf_ptr = self.alloc(
            AllocParams::new(elf_seg.len() * size_of::<Segment>())
                .as_type(RAMType::ElfSegments)
        ).unwrap();

        unsafe {
            core::ptr::copy(efi_ram.as_ptr(), efi_ptr.ptr(), efi_ram.len());
            core::ptr::copy(elf_seg.as_ptr(),elf_ptr.ptr(),elf_seg.len());
        }

        SYSINFO.write().layout_ptr = efi_ptr.addr();
        KINFO.write().seg_ptr = elf_ptr.addr();

        {
            let efi_ram = efi_ram_layout_mut();
            efi_ram.sort_noheap_by_key(|desc| desc.phys_start);
            for desc in efi_ram.iter() {
                if desc.ty != RAMType::Conv {
                    let size = desc.page_count as usize * PAGE_4KIB;
                    let addr = desc.phys_start as usize;
                    let mut ty = desc.ty;

                    #[cfg(target_arch = "x86_64")]
                    if addr < 0x100000 { ty = RAMType::Reserved; }

                    if RECLAMABLE.contains(&desc.ty) {
                        ty = RAMType::Reclaimable;
                    }

                    let block = RAMBlock::new(addr, size, ty, false);
                    self.add(block);
                }
            }
        }

        self.is_init = true;
    }

    fn reclaim(&mut self) {
        use alloc::vec::Vec;
        // Heap allocation is permitted because PhysAlloc::reclaim
        // is called only after heap initialisation and only once.

        let iter = self.blocks_raw().iter().enumerate()
            .filter(|(_, blk)| blk.valid() && blk.ty() == RAMType::Reclaimable)
            .map(|(idx, &blk)| (idx, blk)).collect::<Vec<(usize, RAMBlock)>>();

        for (idx, _) in iter.iter() {
            self.blocks_raw_mut()[*idx].invalidate();
        }

        for (_, block) in iter.iter() { // O(kn)
            let new_blk = RAMBlock::new(
                block.addr(), block.size(),
                RAMType::Conv, false
            );
            // SAFETY: self.add() can never fail because
            // self.count() always decreases after invalidation above.
            self.add(new_blk);
        }
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

    pub fn filtsize(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.blocks_iter().filter(|&block| filter(block) && !NON_RAM.contains(&block.ty()))
            .map(|block| block.size()).sum();
    }

    pub fn filtsize_raw(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.blocks_iter().filter(|&block| filter(block))
            .map(|block| block.size()).sum();
    }

    // fn sort(&mut self) {
    //     self.blocks_raw_mut().sort_noheap_by(|a, b|
    //         match (a.valid(), b.valid()) {
    //             ( true,  true) => a.addr().cmp(&b.addr()),
    //             ( true, false) => Ordering::Less,
    //             (false,  true) => Ordering::Greater,
    //             (false, false) => Ordering::Equal
    //         }
    //     );
    // }

    fn find(&mut self, mut f: impl FnMut(&RAMBlock) -> bool) -> Option<&mut RAMBlock> {
        return self.blocks_iter_mut().find(|block| f(block));
    }

    fn find_free_ram(&mut self, args: AllocParams) -> Option<OwnedPtr> {
        let args = args.build();
        return self.find(|block| {
            let aligned = align_up(block.addr(), args.align);

            block.not_used()
            && aligned + args.size <= block.addr() + block.size()
            && block.size() >= args.size
            && block.ty() == args.from_type
        }
        ).map(|block|{
            let addr = align_up(block.addr(), args.align);
            OwnedPtr::new_bytes(addr, args.size)
        });
    }

    fn alloc(&mut self, args: AllocParams) -> Option<OwnedPtr> {
        let args = args.build();
        if NON_RAM.contains(&args.from_type) || NON_RAM.contains(&args.as_type) {
            return None; // Cannot allocate from or as non-RAM types
        }

        let ptr = match args.addr {
            Some(addr) => OwnedPtr::new_bytes(addr, args.size),
            None => self.find_free_ram(args)?
        };

        if self.count() + MIN_REQ > self.max {
            // Every allocation can split a RAMBlock into 3 parts which will need 2 extra RAMBlocks.
            // as allocation can happen twice (once for expansion, once for request itself)
            // we need at least 4 extra RAMBlocks.
            let new_size = (self.max << 1).max(self.max + MIN_REQ);
            // SAFETY: ptr is cloned for a purpose of metadata, RAM access to ptr will never happen.
            self.expand(new_size, unsafe { ptr.clone() })?;
        }

        let filter = |block: &RAMBlock| {
            block.not_used() && args.from_type == block.ty() &&
            ptr.addr() >= block.addr() && ptr.addr() + ptr.size() <= block.addr() + block.size()
        };

        let (from, to) = self.blocks_iter_mut().find(|block|
            filter(block)
        ).and_then(|block| {
            if block.ty() == args.as_type && !args.used {
                return None; // Allocation lost its purpose
            }

            let from = *block;
            let to = RAMBlock::new(ptr.addr(), ptr.size(), args.as_type, args.used);
            block.invalidate();
            return Some((from, to));
        })?;

        let before_block = RAMBlock::new(
            from.addr(), ptr.addr() - from.addr(),
            from.ty(), false
        );
        let after_block = RAMBlock::new(
            ptr.addr() + ptr.size(),
            from.addr() + from.size() - (ptr.addr() + ptr.size()),
            from.ty(), false
        );
        self.add(before_block);
        self.add(after_block);
        self.add(to);

        return Some(to.into_owned_ptr());
    }

    fn free(&mut self, ptr: OwnedPtr) {
        let found_block = self.find(|block|
            block.addr() <= ptr.addr() && block.addr() + block.size() > ptr.addr()
        );

        if let Some(block) = found_block {
            let block_cp = *block;
            let free_start = ptr.addr();
            let free_end = (ptr.addr() + ptr.size()).min(block_cp.addr() + block_cp.size());
            let free_size = free_end - free_start;

            block.invalidate();
            if block_cp.addr() < free_start {
                let before_size = free_start - block_cp.addr();
                let before_block = RAMBlock::new(block_cp.addr(), before_size, block_cp.ty(), block_cp.used());
                self.add(before_block);
            }
            let this_block = RAMBlock::new(free_start, free_size, RAMType::Conv, false);
            self.add(this_block);
            if free_end < block_cp.addr() + block_cp.size() {
                let after_size = block_cp.addr() + block_cp.size() - free_end;
                let after_block = RAMBlock::new(free_end, after_size, block_cp.ty(), block_cp.used());
                self.add(after_block);
            }
        }
    }

    fn add(&mut self, new_block: RAMBlock) {
        if new_block.invalid() { return; }
        let (mut before, mut after) = (None, None);
        for block in self.blocks_iter_mut() {
            match new_block.is_mergable(&block) {
                -1 => { after = Some(block); },
                1 => { before = Some(block); },
                _ => continue
            }
        }

        match (before, after) {
            (Some(before_block), Some(after_block)) => {
                before_block.set_size(before_block.size() + new_block.size() + after_block.size());
                after_block.invalidate();
                if self.count() <= self.max >> 2 {
                    let new_size = (self.max >> 1).max(BASE_RB_SIZE);
                    self.shrink(new_size);
                }
            },
            (Some(before_block), None) => {
                before_block.set_size(before_block.size() + new_block.size());
            },
            (None, Some(after_block)) => {
                after_block.set_addr(new_block.addr());
                after_block.set_size(after_block.size() + new_block.size());
            },
            (None, None) => {
                let prereq = new_block.into_owned_ptr();
                if self.count() >= self.max {
                    let new_size = (self.max << 1).max(self.max + MIN_REQ);
                    self.expand(new_size, prereq).expect("Failed to expand RAMBlocks");
                }

                let blocks = self.blocks_raw_mut();
                for block in blocks {
                    if block.valid() { continue; }
                    *block = new_block;
                    break;
                }
            }
        }
    }

    fn expand(&mut self, new_max: usize, prereq: OwnedPtr) -> Option<()> {
        if new_max <= self.max { return Some(()); }

        let alloc_param = AllocParams::new(new_max * size_of::<RAMBlock>());
        let old_blocks = unsafe { self.ptr.clone() };

        let p = prereq; // pre-requested ptr(henceforth P)
        let new_blocks = self.find(|block| {
            return {
                block.size() >= alloc_param.size // block is large enough
                && !block.used()                 // block is free
                && block.ty() == alloc_param.from_type // block is Conv
                && {
                    block.addr() + block.size() <= p.addr() // block is before P
                    || block.addr() >= p.addr() + p.size()  // block is after P
                    || p.addr().saturating_sub(block.addr()) >= alloc_param.size
                    || (block.addr() + block.size()).saturating_sub(p.addr() + p.size()) >= alloc_param.size
                    // has enough space before or after P
                }
            };
        }).map(|block| {
            let addr;
            if {
                block.addr() + block.size() <= p.addr() // block is before P
                || block.addr() >= p.addr() + p.size()  // block is after P
                || p.addr().saturating_sub(block.addr()) >= alloc_param.size
                // has enough space before P
            } {
                addr = block.addr();
            } else {
                // has enough space after P
                addr = p.addr() + p.size();
            }

            return OwnedPtr::new_bytes(addr, alloc_param.size);
        })?;

        unsafe {
            new_blocks.ptr::<RAMBlock>().write_bytes(0, new_max);
            old_blocks.ptr::<RAMBlock>().copy_to(new_blocks.ptr::<RAMBlock>(), self.max);
        }
        (self.ptr, self.max) = (new_blocks, new_max);
        self.free(old_blocks);
        self.alloc(alloc_param.at(self.ptr.ptr::<RAMBlock>()));

        return Some(());
    }

    fn shrink(&mut self, new_max: usize) {
        if new_max >= self.max || new_max < self.count() || new_max < MIN_REQ {
            return;
        }

        let blocks_raw = self.blocks_raw_mut();
        let Some((kept, freed)) = blocks_raw.split_at_mut_checked(new_max) else {
            return;
        };

        let mut slots = kept.iter_mut().filter(|block| block.invalid());
        for block in freed.iter_mut().rev() {
            if block.valid() {
                if let Some(slot) = slots.next() {
                    *slot = *block;
                    block.invalidate();
                } else {
                    return;
                }
            }
        }

        let kept_addr = kept.as_ptr() as usize;
        let kept_size = align_up(kept.len() * size_of::<RAMBlock>(), page_size());
        let freed_addr = align_up(kept_addr + kept_size, page_size());
        let freed_size = self.ptr.size() - kept_size;

        if freed_size == 0 { return; }
        let freed_ptr = OwnedPtr::new_bytes(freed_addr, freed_size);

        self.max = new_max;
        self.ptr = OwnedPtr::new_bytes(self.ptr.addr(), kept_size);

        self.free(freed_ptr);
    }
}

impl PhysAllocGlob {
    const fn empty() -> Self {
        return Self(IntLock::new(PhysAlloc::empty()));
    }

    pub fn init(&self) { self.0.lock().init(); }
    pub fn reclaim(&self) { self.0.lock().reclaim(); }

    pub fn filtsize(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.0.lock().filtsize(filter);
    }

    pub fn filtsize_raw(&self, filter: impl Fn(&RAMBlock) -> bool) -> usize {
        return self.0.lock().filtsize_raw(filter);
    }

    pub fn available(&self) -> usize {
        return self.0.lock().filtsize(|block| block.not_used() && block.ty() == RAMType::Conv);
    }

    pub fn total(&self) -> usize {
        return self.0.lock().filtsize(|_| true);
    }

    // pub fn sort(&self) { self.0.lock().sort(); }

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
}
