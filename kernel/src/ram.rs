use crate::{arch, ember::ramtype, ramblock::{AllocParams, RAMBLOCK}};
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
        return Self { ptr, layout };
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

pub fn align_up(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return val + (align - val % align) % align;
}

pub fn init_ram() {
    let stack_ptr = RAMBLOCK.lock().alloc(
        AllocParams::new(STACK_SIZE).as_type(ramtype::KERNEL_DATA)
    ).unwrap();
    unsafe { arch::move_stack(&stack_ptr, STACK_SIZE); }

    let available = RAMBLOCK.lock().available();
    let heap_size = ((available as f64 * 0.02) as usize).max(HEAP_SIZE);
    let heap_ptr = RAMBLOCK.lock().alloc(
        AllocParams::new(heap_size).as_type(ramtype::KERNEL_DATA)
    ).unwrap();
    unsafe { ALLOCATOR.lock().init(heap_ptr.ptr(), heap_size); }
}