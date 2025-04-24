use crate::arch;
use core::ptr::write_volatile;
use linked_list_allocator::LockedHeap;

pub const STACK_SIZE: usize = 0x10_0000;
pub const HEAP_SIZE: usize = 0x10_0000;

pub const PAGE_4KIB: usize = 0x1000;
// pub const PAGE_2MIB: usize = 0x200000;
// pub const PAGE_1GIB: usize = 0x40000000;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMDescriptor {
    pub ty: u32,
    pub reserved: u32,
    pub phys_start: u64,
    pub virt_start: u64,
    pub page_count: u64,
    pub attr: u64,
    pub padding: u64
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMInfo {
    pub base: u64,
    pub size: u64,
    pub available: u64
}

pub fn get_largest_descriptor(efi_ram_layout: &[RAMDescriptor]) -> &RAMDescriptor {
    return efi_ram_layout.iter()
    .filter(|e| e.ty == 7) // CONVENTIONAL = 7
    .max_by_key(|e| e.page_count).unwrap();
}

pub fn get_ram_info(efi_ram_layout: &[RAMDescriptor]) -> RAMInfo {
    let descriptor_largest = get_largest_descriptor(efi_ram_layout);
    let base = descriptor_largest.phys_start;
    let available = descriptor_largest.page_count * PAGE_4KIB as u64;

    let last_ram_desc = efi_ram_layout[efi_ram_layout.len() - 1];
    let size = last_ram_desc.phys_start + last_ram_desc.page_count * PAGE_4KIB as u64;
    return RAMInfo { base, size, available };
}

pub fn init_ram(efi_ram_layout: &[RAMDescriptor]) {
    let raminfo = get_ram_info(efi_ram_layout);
    let available_from = unsafe { arch::identity_map(raminfo) };
    unsafe { arch::move_stack(raminfo, STACK_SIZE); }
    if raminfo.available < HEAP_SIZE as u64 { panic!("Not enough RAM for heap"); }

    unsafe {
        let ptr = available_from as *mut u8;
        arch::serial_print("Heap end: ");
        arch::print_u64(ptr.add(HEAP_SIZE) as u64);
        arch::serial_print("\nRSP: ");
        arch::print_u64(arch::rsp() as u64);
        arch::serial_print("\n");
        for i in 0..HEAP_SIZE { write_volatile(ptr.add(i), 0); }
        core::ptr::write_bytes(ptr, 0, HEAP_SIZE);
        arch::serial_print("Heap init...");
        ALLOCATOR.lock().init(ptr, HEAP_SIZE);
    }
    arch::serial_print(" OK\n");
}