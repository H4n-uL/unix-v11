use crate::{ember::Ember, ram::{align_up, RAMInfo, PAGE_4KIB}};
use core::{fmt, ptr::{copy, write_bytes}};
use x86_64::{
    instructions::{hlt, interrupts, port::Port, tlb},
    registers::control::{Cr0, Cr0Flags, Cr3, Cr3Flags, Cr4, Cr4Flags, Efer, EferFlags},
    structures::paging::PhysFrame,
    PhysAddr
};

pub fn halt() {
    interrupts::disable();
    hlt();
}

const COM1: u16 = 0x3f8;

pub fn init_serial() {
    unsafe {
        Port::new(COM1 + 1).write(0x00u8); // Disable all interrupts
        Port::new(COM1 + 3).write(0x80u8); // Enable DLAB (set baud rate divisor)
        Port::new(COM1 + 0).write(0x03u8); // Set divisor to 3 (lo byte) 38400 baud
        Port::new(COM1 + 1).write(0x00u8); //                  (hi byte)
        Port::new(COM1 + 3).write(0x03u8); // 8 bits, no parity, one stop bit
        Port::new(COM1 + 2).write(0xc7u8); // Enable FIFO, clear them, with 14-byte threshold
        Port::new(COM1 + 4).write(0x0bu8); // IRQs enabled, RTS/DSR set
    }
}

pub fn serial_putchar(byte: u8) {
    unsafe {
        while Port::<u8>::new(COM1 + 5).read() & 0x20 == 0 {}
        Port::<u8>::new(COM1).write(byte);
    }
}

pub struct SerialWriter;

impl fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            serial_putchar(byte);
        }
        Ok(())
    }
}

const ENTRIES_PER_TABLE: usize = 0x200;
const NORMAL_FLAG: u64 = 0x03; // PRESENT | WRITABLE
const MMIO_FLAG: u64 = 0x1b; // PRESENT | WRITABLE | PWT | PCD

pub unsafe fn identity_map(ember: &Ember) -> usize {
    let last_desc = ember.efi_ram_layout().iter()
        .max_by_key(|desc| desc.phys_start).unwrap();
    let ram_size = last_desc.phys_start + last_desc.page_count * PAGE_4KIB as u64;

    // Enable PAE, PSE, and Long mode
    Cr4::write(Cr4::read() | Cr4Flags::PHYSICAL_ADDRESS_EXTENSION | Cr4Flags::PAGE_SIZE_EXTENSION);
    Efer::write(Efer::read() | EferFlags::LONG_MODE_ENABLE | EferFlags::NO_EXECUTE_ENABLE);

    // Calculate page table counts, sizes, and base addresses
    let num_4kib_pages = (ram_size as usize + PAGE_4KIB - 1) / PAGE_4KIB;
    let num_pt = (num_4kib_pages + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_pd = (num_pt + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_pdpt = (num_pd + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;

    let pml4_addr = align_up(ember.kernel_base + ember.kernel_size, PAGE_4KIB) as u64;
    let pdpt_base = pml4_addr + PAGE_4KIB as u64;
    let pd_base = pdpt_base + (num_pdpt as u64 * PAGE_4KIB as u64);
    let pt_base = pd_base + (num_pd as u64 * PAGE_4KIB as u64);

    let table_size = (1 + num_pdpt + num_pd + num_pt) * PAGE_4KIB;

    write_bytes(pml4_addr as *mut u8, 0, table_size);

    for i in 0..num_pdpt { // Link PML4 -> PDPTs
        let pml4_entry = (pml4_addr + (i as u64 * 8)) as *mut u64;
        *pml4_entry = (pdpt_base + (i as u64 * PAGE_4KIB as u64)) | 0x3;
    }

    for i in 0..num_pdpt { // Link PDPTs -> PDs
        let pdpt_entry_addr = pdpt_base + (i as u64 * PAGE_4KIB as u64);
        for j in 0..ENTRIES_PER_TABLE {
            let pdpt_entry = (pdpt_entry_addr + (j as u64 * 8)) as *mut u64;
            let pd_index = i * ENTRIES_PER_TABLE + j;
            if pd_index >= num_pd { break; }
            *pdpt_entry = (pd_base + (pd_index as u64 * PAGE_4KIB as u64)) | 0x3;
        }
    }

    for i in 0..num_pd { // Link PDs -> PTs
        let pd_entry_addr = pd_base + (i as u64 * PAGE_4KIB as u64);
        for j in 0..ENTRIES_PER_TABLE {
            let pd_entry = (pd_entry_addr + (j as u64 * 8)) as *mut u64;
            let pt_index = i * ENTRIES_PER_TABLE + j;
            if pt_index >= num_pt { break; }
            *pd_entry = (pt_base + (pt_index as u64 * PAGE_4KIB as u64)) | 0x3;
        }
    }

    let mut phys = 0u64;
    for pt_idx in 0..num_pt { // Allocate Pages (Identity Mapping)
        let pt_table_addr = pt_base + (pt_idx as u64 * PAGE_4KIB as u64);
        for j in 0..ENTRIES_PER_TABLE {
            if phys >= ram_size { break; }
            let entry = (pt_table_addr + (j as u64 * 8)) as *mut u64;
            *entry = phys | NORMAL_FLAG;
            phys += PAGE_4KIB as u64;
        }
    }

    // Register PML4 in CR3
    Cr3::write(
        PhysFrame::containing_address(PhysAddr::new(pml4_addr)),
        Cr3Flags::empty()
    );

    // Warrant that paging is enabled
    Cr0::write(Cr0::read() | Cr0Flags::PAGING);

    // Flush TLB
    tlb::flush_all();
    return pml4_addr as usize + table_size;
}

#[inline(always)]
pub fn stack_ptr() -> usize {
    let rsp: usize;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp); }
    return rsp;
}

pub unsafe fn move_stack(ember: &mut Ember, raminfo: RAMInfo) {
    let current_sp = stack_ptr();
    let old_stack_base = ember.stack_base;
    let stack_size = old_stack_base - current_sp;

    let new_stack_base = (raminfo.base + raminfo.available) as usize;
    let new_stack_bottom = new_stack_base - stack_size;

    copy(current_sp as *const u8, new_stack_bottom as *mut u8, stack_size);
    core::arch::asm!("mov rsp, {}", in(reg) new_stack_bottom);

    ember.set_new_stack_base(new_stack_base);
}