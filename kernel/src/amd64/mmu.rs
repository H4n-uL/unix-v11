use crate::{glacier::{AllocParams, GLACIER}, ram::PAGE_4KIB, sysinfo::ramtype, SYS_INFO};

// const UNAVAILABLE_FLAG: u64 = 0x01; // PRESENT
const KERNEL_FLAG: u64 = 0x03;      // PRESENT | WRITABLE
const NORMAL_FLAG: u64 = 0x07;      // PRESENT | WRITABLE | USER
const PROTECT_FLAG: u64 = 0x1b;     // PRESENT | WRITABLE |      | PWT | PCD

pub unsafe fn map_page(pml4: *mut u64, virt: u64, phys: u64, flags: u64) {
    let virt = virt & 0x000fffff_fffff000;
    let phys = phys & 0x000fffff_fffff000;

    fn get_index(level: usize, virt: u64) -> usize {
        match level {
            0 => ((virt >> 39) & 0x1ff) as usize, // PML4
            1 => ((virt >> 30) & 0x1ff) as usize, // PDPT
            2 => ((virt >> 21) & 0x1ff) as usize, // PD
            3 => ((virt >> 12) & 0x1ff) as usize, // PT
            _ => unreachable!()
        }
    }

    let mut table = pml4;
    for level in 0..4 {
        let index = get_index(level, virt);
        let entry = unsafe { table.add(index) };
        if level == 3 { unsafe { *entry = phys | flags; } }
        else {
            table = unsafe { if *entry & 0x1 == 0 {
                let next_phys = GLACIER.alloc(AllocParams::new(PAGE_4KIB).as_type(ramtype::PAGE_TABLE))
                    .expect("[ERROR] alloc for page table failed!");
                core::ptr::write_bytes(next_phys.ptr::<*mut u8>(), 0, PAGE_4KIB);
                *entry = next_phys.addr() as u64 | KERNEL_FLAG;
                next_phys.ptr()
            }
            else { (*entry & 0x000fffff_fffff000) as *mut u64 } };
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

pub unsafe fn identity_map() {
    let pml4_addr = GLACIER.alloc(AllocParams::new(PAGE_4KIB).as_type(ramtype::PAGE_TABLE)).unwrap();
    unsafe { core::ptr::write_bytes(pml4_addr.ptr::<*mut u8>(), 0, PAGE_4KIB); }

    // Map Page Tables
    for desc in SYS_INFO.lock().efi_ram_layout() {
        let block_ty = desc.ty;
        let block_start = desc.phys_start;
        let block_end = block_start + desc.page_count * PAGE_4KIB as u64;

        for phys in (block_start..block_end).step_by(PAGE_4KIB) {
            unsafe { map_page(pml4_addr.ptr(), phys, phys, flags_for(block_ty)); }
        }
    }

    unsafe {
        core::arch::asm!(
            // Register PML4 in CR3
            "mov rax, {pml4_addr}",
            "mov cr3, rax",

            // Set Paging bit
            "mov rax, cr0",
            "or eax, 0x80000000",
            "mov cr0, rax",

            // Set PAE and PSE in CR4
            "mov rax, cr4",
            "or rax, 0x00000020", // Set PAE
            "or rax, 0x00000010", // Set PSE
            "mov cr4, rax",

            // Enable Long Mode in EFER
            "mov ecx, 0xC0000080", // EFER MSR number
            "rdmsr",
            "or eax, 0x00000100", // Set Long Mode Enable
            "or eax, 0x00000800", // Set No-Execute Enable
            "wrmsr",
            pml4_addr = in(reg) pml4_addr.addr(),
            options(nostack)
        );
    }
}

pub fn id_map_ptr() -> *const u8 {
    let id_map_ptr: usize;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) id_map_ptr); }
    return (id_map_ptr & !0xfff) as *const u8;
}