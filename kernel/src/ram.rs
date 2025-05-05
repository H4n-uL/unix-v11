use crate::{arch, ember::{ramtype, Ember}, ramblock::RAM_BLOCK_MANAGER};
use linked_list_allocator::LockedHeap;

pub const STACK_SIZE: usize = 0x100000;
pub const HEAP_SIZE: usize = 0x100000;

pub const PAGE_4KIB: usize = 0x1000;
// pub const PAGE_2MIB: usize = 0x200000;
// pub const PAGE_1GIB: usize = 0x40000000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init_ram(ember: &Ember) {
    let mut ramblock = RAM_BLOCK_MANAGER.lock();
    unsafe { arch::identity_map(ember, &mut ramblock); }
    let stack_ptr = ramblock.alloc(STACK_SIZE, ramtype::CONVENTIONAL).unwrap();
    ramblock.from_addr_mut(stack_ptr).unwrap().set_ty(ramtype::KERNEL_DATA);
    unsafe { arch::move_stack(stack_ptr, STACK_SIZE); }
    let available = ramblock.available();
    let heap_size = ((available as f64 * 0.02) as usize).max(HEAP_SIZE);
    let heap_ptr = ramblock.alloc(heap_size, ramtype::CONVENTIONAL).unwrap();
    ramblock.from_addr_mut(heap_ptr).unwrap().set_ty(ramtype::KERNEL_DATA);
    unsafe { ALLOCATOR.lock().init(heap_ptr, heap_size); }
}