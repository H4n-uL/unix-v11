pub mod glacier;
pub mod physalloc;

use crate::{
    arch::move_stack,
    ram::physalloc::{AllocParams, PHYS_ALLOC},
    sysinfo::ramtype
};
use core::{alloc::Layout, ops::{Deref, DerefMut}};
use alloc::alloc::{alloc, dealloc};
use spin::Mutex;
use talc::{ErrOnOom, Talc, Talck};

pub const PAGE_4KIB: usize = 0x1000;
pub const STACK_SIZE: usize = 0x100000;
pub const HEAP_SIZE: usize = 0x100000;

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
static ALLOCATOR: Talck<Mutex<()>, ErrOnOom> = Talc::new(ErrOnOom).lock();

pub fn align_up(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return val.div_ceil(align) * align;
}

pub fn init_ram() {
    let mut phys_alloc = PHYS_ALLOC.lock();
    let stack_ptr = phys_alloc.alloc(
        AllocParams::new(STACK_SIZE).as_type(ramtype::KERNEL_DATA)
    ).unwrap();
    unsafe { move_stack(&stack_ptr); }

    let available = phys_alloc.available();
    let heap_size = ((available as f64 * 0.05) as usize).max(HEAP_SIZE);
    let heap_ptr = phys_alloc.alloc(
        AllocParams::new(heap_size).as_type(ramtype::KERNEL_DATA)
    ).unwrap();
    unsafe { ALLOCATOR.lock().claim(heap_ptr.into_slice::<u8>().into()).unwrap(); }
}