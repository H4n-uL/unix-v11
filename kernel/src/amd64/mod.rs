use crate::{kernel_size, ram::{RAMInfo, PAGE_4KIB}};
use core::cmp::Ordering;
use x86_64::{
    instructions::{hlt, interrupts, tlb},
    registers::control::{Cr0, Cr0Flags, Cr3, Cr3Flags, Cr4, Cr4Flags, Efer, EferFlags},
    structures::paging::PhysFrame,
    PhysAddr
};

pub fn halt() {
    interrupts::disable();
    hlt();
}

const COM1: u16 = 0x3F8;

pub fn init_serial() {
    unsafe {
        use x86_64::instructions::port::Port;

        let mut port = Port::new(COM1 + 1);
        port.write(0x00u8); // Disable all interrupts
        let mut port = Port::new(COM1 + 3);
        port.write(0x80u8); // Enable DLAB (set baud rate divisor)
        let mut port = Port::new(COM1 + 0);
        port.write(0x03u8); // Set divisor to 3 (lo byte) 38400 baud
        let mut port = Port::new(COM1 + 1);
        port.write(0x00u8); //                  (hi byte)
        let mut port = Port::new(COM1 + 3);
        port.write(0x03u8); // 8 bits, no parity, one stop bit
        let mut port = Port::new(COM1 + 2);
        port.write(0xC7u8); // Enable FIFO, clear them, with 14-byte threshold
        let mut port = Port::new(COM1 + 4);
        port.write(0x0Bu8); // IRQs enabled, RTS/DSR set
    }
}

pub fn serial_write_byte(byte: u8) {
    unsafe {
        use x86_64::instructions::port::Port;

        let mut line_status = Port::<u8>::new(COM1 + 5);
        while line_status.read() & 0x20 == 0 {}

        let mut data = Port::<u8>::new(COM1);
        data.write(byte);
    }
}

pub fn serial_print(s: &str) {
    for b in s.bytes() {
        serial_write_byte(b);
    }
}

pub fn print_hex64(num: u64) {
    (0..16).rev().for_each(|i| {
        let nibble = (num >> (i * 4)) & 0xF;
        let hex_char = b"0123456789abcdef"[nibble as usize];
        serial_write_byte(hex_char);
    });
}

pub fn print_u64(num: u64) {
    let mut num = num;
    let mut digits = [0u8; 20];
    let mut i = 0;

    if num == 0 {
        serial_write_byte(b'0');
        return;
    }

    while num > 0 {
        digits[i] = (num % 10) as u8 + b'0';
        num /= 10;
        i += 1;
    }

    for j in (0..i).rev() {
        serial_write_byte(digits[j]);
    }
}

const ENTRIES_PER_TABLE: usize = 0x200;

pub unsafe fn identity_map(raminfo: RAMInfo) -> usize {
    // Enable PAE and PSE
    let mut cr4 = Cr4::read();
    cr4 |= Cr4Flags::PHYSICAL_ADDRESS_EXTENSION | Cr4Flags::PAGE_SIZE_EXTENSION;
    Cr4::write(cr4);

    // Enable long mode
    let mut efer = Efer::read();
    efer |= EferFlags::LONG_MODE_ENABLE | EferFlags::NO_EXECUTE_ENABLE;
    Efer::write(efer);

    let pml4_addr = kernel_size() as u64 + raminfo.base;

    // Calculate page table counts, sizes, and base addresses
    let num_4kib_pages = (raminfo.size as usize + PAGE_4KIB - 1) / PAGE_4KIB;
    let num_pt = (num_4kib_pages + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_pd = (num_pt + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_pdpt = (num_pd + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;

    let table_size = (1 + num_pdpt + num_pd + num_pt) * PAGE_4KIB;

    let pdpt_base = pml4_addr + PAGE_4KIB as u64;
    let pd_base = pdpt_base + (num_pdpt as u64 * PAGE_4KIB as u64);
    let pt_base = pd_base + (num_pd as u64 * PAGE_4KIB as u64);

    for i in 0..ENTRIES_PER_TABLE { // Zero out PML4
        let entry = (pml4_addr + (i as u64 * 8)) as *mut u64;
        *entry = 0;
    }

    for t in 0..num_pdpt { // Zero out PDPTs
        let table_addr = pdpt_base + (t as u64 * PAGE_4KIB as u64);
        for i in 0..ENTRIES_PER_TABLE {
            let entry = (table_addr + (i as u64 * 8)) as *mut u64;
            *entry = 0;
        }
    }

    for t in 0..num_pd { // Zero out PDs
        let table_addr = pd_base + (t as u64 * PAGE_4KIB as u64);
        for i in 0..ENTRIES_PER_TABLE {
            let entry = (table_addr + (i as u64 * 8)) as *mut u64;
            *entry = 0;
        }
    }

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
            if phys >= raminfo.size { break; }
            let entry = (pt_table_addr + (j as u64 * 8)) as *mut u64;
            *entry = phys | 0x03; // PRESENT | WRITABLE
            phys += PAGE_4KIB as u64;
        }
    }

    // Register PML4 in CR3
    Cr3::write(
        PhysFrame::containing_address(PhysAddr::new(pml4_addr)),
        Cr3Flags::empty()
    );

    // Warrant that paging is enabled
    let mut cr0 = Cr0::read();
    cr0 |= Cr0Flags::PAGING;
    Cr0::write(cr0);

    // Flush TLB
    tlb::flush_all();
    return pml4_addr as usize + table_size;
}

pub fn rsp() -> usize {
    let rsp: usize;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp); }
    return rsp;
}

pub unsafe fn move_stack(raminfo: RAMInfo, stack_size: usize) {
    let stack_src = (rsp() - stack_size) as *const u8;
    let stack_dst = (raminfo.base + raminfo.available - stack_size as u64) as *mut u8;

    match (stack_src as usize).cmp(&(stack_dst as usize)) {
        Ordering::Greater => { for i in 0..stack_size {
            *stack_dst.add(i) = *stack_src.add(i);
        }}
        Ordering::Less => { for i in (0..stack_size).rev() {
            *stack_dst.add(i) = *stack_src.add(i);
        }}
        Ordering::Equal => {}
    }

    core::arch::asm!("mov rsp, {}", in(reg) stack_dst);
}