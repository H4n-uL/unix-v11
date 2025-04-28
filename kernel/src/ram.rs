use crate::{arch, ember::{ramtype, Ember, RAMDescriptor}, ramblock::RAM_BLOCK_MANAGER};
use linked_list_allocator::LockedHeap;

pub const HEAP_SIZE: usize = 0x10_0000;

pub const PAGE_4KIB: usize = 0x1000;
// pub const PAGE_2MIB: usize = 0x200000;
// pub const PAGE_1GIB: usize = 0x40000000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ConvInfo {
    pub conv_base: u64,
    pub conv_size: u64,
    pub conv_available: u64
}

pub struct MappingInfo {
    pub mmu_base: usize,
    pub mmu_size: usize
}

impl MappingInfo {
    pub unsafe fn new(ember: &Ember) -> Self {
        return arch::identity_map(ember);
    }
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

pub fn get_ram_info(efi_ram_layout: &[RAMDescriptor]) -> ConvInfo {
    let descriptor_largest = get_largest_descriptor(efi_ram_layout);
    let last_ram_desc = get_last_descriptor(efi_ram_layout);
    let conv_base = descriptor_largest.phys_start;
    let conv_available = descriptor_largest.page_count * PAGE_4KIB as u64;
    let conv_size = last_ram_desc.phys_start + last_ram_desc.page_count * PAGE_4KIB as u64;
    return ConvInfo { conv_base, conv_size, conv_available };
}

pub fn init_ram(ember: &Ember) {
    let ram_layout = ember.efi_ram_layout();
    RAM_BLOCK_MANAGER.lock().init(ram_layout);
    let conv_info = get_ram_info(ram_layout);
    // let mapinfo = unsafe { MappingInfo::new(ember) };
    // let mapinfo = unsafe { arch::identity_map(ember) };
    unsafe { arch::move_stack(conv_info); }
    if conv_info.conv_available < HEAP_SIZE as u64 { panic!("Not enough RAM for heap"); }
    let available_from = align_up(ember.kernel_base + ember.kernel_size, PAGE_4KIB);
    // let available_from = align_up(mapinfo.mmu_base + mapinfo.mmu_size, PAGE_4KIB);
    unsafe { ALLOCATOR.lock().init(available_from as *mut u8, HEAP_SIZE); }
}