use crate::{ember::ramtype, ram::PAGE_4KIB, ramblock::RAMBlockManager, EMBER};
use core::fmt;
use spin::MutexGuard;
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

pub fn serial_puts(s: &str) {
    for byte in s.bytes() { serial_putchar(byte); }
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

// const UNAVAILABLE_FLAG: u64 = 0x01; // PRESENT
const KERNEL_FLAG: u64 = 0x03;      // PRESENT | WRITABLE
const NORMAL_FLAG: u64 = 0x07;      // PRESENT | WRITABLE | USER
const PROTECT_FLAG: u64 = 0x1b;     // PRESENT | WRITABLE |      | PWT | PCD

pub unsafe fn map_page(pml4: *mut u64, virt: u64, phys: u64, flags: u64, ramblock: &mut RAMBlockManager) {
    let virt = virt & 0x000fffff_fffff000;
    let phys = phys & 0x000fffff_fffff000;

    fn get_index(level: usize, virt: u64) -> usize {
        match level {
            0 => ((virt >> 39) & 0x1FF) as usize, // PML4
            1 => ((virt >> 30) & 0x1FF) as usize, // PDPT
            2 => ((virt >> 21) & 0x1FF) as usize, // PD
            3 => ((virt >> 12) & 0x1FF) as usize, // PT
            _ => panic!("Invalid page table level"),
        }
    }

    let mut table = pml4;
    for level in 0..4 {
        let index = get_index(level, virt);
        let entry = table.add(index);
        if level == 3 { *entry = phys | flags; }
        else {
            table = if *entry & 0x1 == 0 {
                let next_phys = ramblock.alloc(PAGE_4KIB, ramtype::PAGE_TABLE)
                    .expect("[ERROR] alloc for page table failed!\n");
                core::ptr::write_bytes(next_phys, 0, PAGE_4KIB);
                *entry = next_phys as u64 | KERNEL_FLAG;
                next_phys as *mut u64
            }
            else { (*entry & 0x000fffff_fffff000) as *mut u64 };
        }
    }
}

fn flags_for(ty: u32) -> u64 {
    match ty {
        ramtype::CONVENTIONAL => NORMAL_FLAG,
        ramtype::KERNEL =>       KERNEL_FLAG,
        ramtype::KERNEL_DATA =>  KERNEL_FLAG,
        ramtype::PAGE_TABLE =>   KERNEL_FLAG,
        ramtype::MMIO =>         PROTECT_FLAG,
        _ =>                     PROTECT_FLAG
    }
}

pub unsafe fn identity_map(ramblock: &mut MutexGuard<'_, RAMBlockManager>) {
    let ember = EMBER.lock();
    let ram_size = ember.layout_total() as u64;

    // Enable PAE, PSE, and Long mode
    Cr4::write(Cr4::read() | Cr4Flags::PHYSICAL_ADDRESS_EXTENSION | Cr4Flags::PAGE_SIZE_EXTENSION);
    Efer::write(Efer::read() | EferFlags::LONG_MODE_ENABLE | EferFlags::NO_EXECUTE_ENABLE);

    // Calculate page table counts, sizes, and base addresses
    let num_4kib_pages = (ram_size as usize + PAGE_4KIB - 1) / PAGE_4KIB;
    let num_pt = (num_4kib_pages + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_pd = (num_pt + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_pdpt = (num_pd + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;

    let total_tables = 1 + num_pdpt + num_pd + num_pt;
    let table_size = (total_tables * 3) * PAGE_4KIB;
    let pml4_addr = ramblock.reserve_as(table_size, ramtype::CONVENTIONAL, ramtype::PAGE_TABLE, false).unwrap() as u64;
    core::ptr::write_bytes(pml4_addr as *mut u8, 0, table_size);

    // Map Page Tables
    for desc in ember.ram_layout() {
        let block_ty = desc.ty;
        let block_start = desc.phys_start;
        let block_end = block_start + desc.page_count * PAGE_4KIB as u64;

        for phys in (block_start..block_end).step_by(PAGE_4KIB) {
            map_page(pml4_addr as *mut u64, phys, phys, flags_for(block_ty), ramblock);
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
}

#[inline(always)]
pub fn stack_ptr() -> *const u8 {
    let rsp: usize;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp); }
    return rsp as *const u8;
}

pub unsafe fn move_stack(ptr: *mut u8, size: usize) {
    let mut ember = EMBER.lock();
    let stack_ptr = stack_ptr();
    let old_stack_base = ember.stack_base;
    let stack_size = old_stack_base - stack_ptr as usize;

    let new_stack_base = ptr as usize + size;
    let new_stack_bottom = (new_stack_base - stack_size) as *mut u8;

    core::ptr::copy(stack_ptr, new_stack_bottom, stack_size);
    core::arch::asm!("mov rsp, {}", in(reg) new_stack_bottom);

    ember.stack_base = new_stack_base;
}