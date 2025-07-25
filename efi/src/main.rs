//!                              EFI Bootloader                              !//
//!
//! Crafted by HaמuL in 2025
//! Description: EFI Bootloader of UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]

mod arch;
mod sysinfo;

use core::panic::PanicInfo;
use sysinfo::{RAMDescriptor, SysInfo};
use uefi::{
    boot::{allocate_pages, exit_boot_services, get_image_file_system, image_handle, AllocateType, MemoryType},
    cstr16, entry, mem::memory_map::MemoryMap, println,
    proto::media::file::{File, FileAttribute, FileInfo, FileMode},
    table::{cfg, system_table_raw}, Status
};
use xmas_elf::{program::Type, ElfFile};

const PAGE_4KIB: usize = 0x1000;

#[repr(C)]
pub struct RelaEntry {
    offset: u64,
    info: u64,
    addend: u64
}

#[cfg(target_arch = "x86_64")]  const R_RELATIVE: u64 = 8;
#[cfg(target_arch = "aarch64")] const R_RELATIVE: u64 = 1027;
#[cfg(target_arch = "riscv64")] const R_RELATIVE: u64 = 3;

pub fn align_up(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return val.div_ceil(align) * align;
}

#[entry]
fn spark() -> Status {
    let systemtable = system_table_raw().unwrap();
    let (mut acpi_ptr, mut dtb_ptr) = (0, 0);
    unsafe {
        let config_ptr = systemtable.as_ref().configuration_table;
        let config_size = systemtable.as_ref().number_of_configuration_table_entries;
        let config = core::slice::from_raw_parts(config_ptr, config_size);

        for cfg in config.iter() {
            let isacpi = cfg.vendor_guid == cfg::ACPI_GUID && acpi_ptr == 0;
            let isacpi2 = cfg.vendor_guid == cfg::ACPI2_GUID;
            let isdtb = cfg.vendor_guid == cfg::SMBIOS3_GUID;
            if isacpi || isacpi2 { acpi_ptr = cfg.vendor_table as usize; }
            if isdtb             { dtb_ptr  = cfg.vendor_table as usize; }
        }
    }

    let mut filesys_protocol = get_image_file_system(image_handle()).unwrap();
    let mut root = filesys_protocol.open_volume().unwrap();

    let mut file = root.open(
        cstr16!("\\unix"), FileMode::Read, FileAttribute::empty()
    ).unwrap().into_regular_file().unwrap();

    let mut info_buf = [0u8; 512];
    let info = file.get_info::<FileInfo>(&mut info_buf).unwrap();
    let file_size = info.file_size() as usize;

    let file_pages = align_up(file_size, PAGE_4KIB) / PAGE_4KIB;
    let file_ptr = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, file_pages).unwrap();
    let file_binary = unsafe { core::slice::from_raw_parts_mut(file_ptr.as_ptr(), file_size) };
    file.read(file_binary).unwrap();

    let elf = ElfFile::new(file_binary).unwrap();

    let kernel_size = elf.program_iter()
        .filter(|ph| ph.get_type() == Ok(Type::Load))
        .map(|ph| ph.virtual_addr() + ph.mem_size())
        .max().unwrap() as usize;

    let kernel_pages = align_up(kernel_size, PAGE_4KIB) / PAGE_4KIB;
    let kernel_base = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_CODE, kernel_pages).unwrap().as_ptr() as usize;

    for ph in elf.program_iter() {
        if let Ok(Type::Load) = ph.get_type() {
            let offset = ph.offset() as usize;
            let file_size = ph.file_size() as usize;
            let mem_size = ph.mem_size() as usize;
            let phys_addr = (kernel_base + ph.virtual_addr() as usize) as *mut u8;

            unsafe {
                core::ptr::copy(file_binary[offset..offset + file_size].as_ptr(), phys_addr, file_size);
                core::ptr::write_bytes(phys_addr.add(file_size), 0, mem_size - file_size);
            }
        }
    }

    let rela = elf.find_section_by_name(".rela.dyn").unwrap();
    let rela_ptr = (kernel_base + rela.address() as usize) as *mut RelaEntry;
    let entry_count = rela.size() as usize / size_of::<RelaEntry>();
    for i in 0..entry_count {
        let entry = unsafe { &*rela_ptr.add(i) };
        let ty = entry.info & 0xffffffff;
        if ty == R_RELATIVE {
            let reloc_addr = (kernel_base + entry.offset as usize) as *mut u64;
            unsafe { *reloc_addr = kernel_base as u64 + entry.addend; }
        }
    }

    let entrypoint = elf.header.pt2.entry_point() as usize + kernel_base;
    let ignite: extern "efiapi" fn(SysInfo) -> ! = unsafe { core::mem::transmute(entrypoint) };
    let efi_ram_layout = unsafe { exit_boot_services(Some(MemoryType::LOADER_DATA)) };
    let stack_base = arch::stack_ptr();
    let sysinfo = SysInfo {
        layout_ptr: efi_ram_layout.buffer().as_ptr() as *const RAMDescriptor,
        layout_len: efi_ram_layout.len(),
        acpi_ptr, dtb_ptr,
        stack_base, kernel_base, kernel_size
    };
    ignite(sysinfo);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("Panic: {}", info);
    loop { arch::halt(); }
}