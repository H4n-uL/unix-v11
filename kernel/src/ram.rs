use crate::{arch, ember::{Ember, RAMDescriptor, ramtype}};
use linked_list_allocator::LockedHeap;

pub const HEAP_SIZE: usize = 0x10_0000;

pub const PAGE_4KIB: usize = 0x1000;
// pub const PAGE_2MIB: usize = 0x200000;
// pub const PAGE_1GIB: usize = 0x40000000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMInfo {
    pub base: u64,
    pub size: u64,
    pub available: u64
}

pub fn align_up(ptr: usize, align: usize) -> usize {
    let mask = align - 1;
    return (ptr + mask) & !mask;
}

pub fn get_largest_descriptor(efi_ram_layout: &[RAMDescriptor]) -> &RAMDescriptor {
    return efi_ram_layout.iter()
    .filter(|e| e.ty == ramtype::CONVENTIONAL) // Convetional
    .max_by_key(|e| e.page_count).unwrap();
}

pub fn get_last_descriptor(efi_ram_layout: &[RAMDescriptor]) -> &RAMDescriptor {
    return efi_ram_layout.iter().max_by_key(|e| e.phys_start).unwrap();
}

pub fn get_ram_info(efi_ram_layout: &[RAMDescriptor]) -> RAMInfo {
    let descriptor_largest = get_largest_descriptor(efi_ram_layout);
    let last_ram_desc = get_last_descriptor(efi_ram_layout);
    let base = descriptor_largest.phys_start;
    let available = descriptor_largest.page_count * PAGE_4KIB as u64;
    let size = last_ram_desc.phys_start + last_ram_desc.page_count * PAGE_4KIB as u64;
    return RAMInfo { base, size, available };
}

pub fn init_ram(ember: &mut Ember) {
    let raminfo = get_ram_info(ember.efi_ram_layout());
    let available_from = unsafe { arch::identity_map(ember) };
    unsafe { arch::move_stack(ember, raminfo); }
    if raminfo.available < HEAP_SIZE as u64 { panic!("Not enough RAM for heap"); }
    unsafe { ALLOCATOR.lock().init(available_from as *mut u8, HEAP_SIZE); }
}