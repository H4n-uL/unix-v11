use crate::{arch, ember::ramtype, ramblock::RAM_BLOCK_MANAGER};
use core::{alloc::Layout, ops::{Deref, DerefMut}};
use alloc::alloc::{alloc, dealloc};
use linked_list_allocator::LockedHeap;

pub const STACK_SIZE: usize = 0x100000;
pub const HEAP_SIZE: usize = 0x100000;

pub const PAGE_4KIB: usize = 0x1000;

pub struct PageAligned {
    ptr: *mut u8,
    layout: Layout
}

impl PageAligned {
    pub fn new(size: usize) -> Self {
        let layout = Layout::from_size_align(size, PAGE_4KIB).unwrap();
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() { panic!("Failed to allocate aligned memory"); }
        Self { ptr, layout }
    }
}

impl Drop for PageAligned {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout); }
    }
}

impl Deref for PageAligned {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }
}

impl DerefMut for PageAligned {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn align_up(size: usize, align: usize) -> usize {
    if align == 0 { return size; }
    return size + (align - size % align) % align;
}

pub fn init_ram() {
    let mut ramblock = RAM_BLOCK_MANAGER.lock();

    let stack_ptr = ramblock.alloc_as(
        STACK_SIZE, ramtype::CONVENTIONAL, ramtype::KERNEL_DATA
    ).unwrap();
    unsafe { arch::move_stack(&stack_ptr, STACK_SIZE); }

    let available = ramblock.available();
    let heap_size = ((available as f64 * 0.02) as usize).max(HEAP_SIZE);
    let heap_ptr = ramblock.alloc_as(
        heap_size, ramtype::CONVENTIONAL, ramtype::KERNEL_DATA
    ).unwrap();
    unsafe { ALLOCATOR.lock().init(heap_ptr.ptr(), heap_size); }
}