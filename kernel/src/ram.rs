use crate::{arch, ember::ramtype, ramblock::RAM_BLOCK_MANAGER};
use linked_list_allocator::LockedHeap;

pub const STACK_SIZE: usize = 0x100000;
pub const HEAP_SIZE: usize = 0x100000;

pub const PAGE_4KIB: usize = 0x1000;
// pub const PAGE_2MIB: usize = 0x200000;
// pub const PAGE_1GIB: usize = 0x40000000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init_ram() {
    let mut ramblock = RAM_BLOCK_MANAGER.lock();
    unsafe { arch::identity_map(&mut ramblock); }
    let stack_ptr = ramblock.alloc_as(
        STACK_SIZE, ramtype::CONVENTIONAL, ramtype::KERNEL_DATA
    ).unwrap();
    unsafe { arch::move_stack(&stack_ptr, STACK_SIZE); }
}

pub fn init_heap() {
    let mut ramblock = RAM_BLOCK_MANAGER.lock();
    let available = ramblock.available();
    let heap_size = ((available as f64 * 0.02) as usize).max(HEAP_SIZE);
    let heap_ptr = ramblock.alloc_as(
        heap_size, ramtype::CONVENTIONAL, ramtype::KERNEL_DATA
    ).unwrap();
    unsafe { ALLOCATOR.lock().init(heap_ptr.ptr(), heap_size); }
}