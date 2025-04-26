//!                              EFI Bootloader                              !//
//!
//! Crafted by HaמuL in 2025
//! Description: EFI Bootloader of Research UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]

mod ember;

use core::{panic::PanicInfo, ptr::{copy, write_bytes}, slice::from_raw_parts_mut};
use ember::{Ember, RAMDescriptor};
use uefi::{
    boot::{allocate_pages, exit_boot_services, get_image_file_system, image_handle, memory_map, AllocateType, MemoryType},
    cstr16, entry, mem::memory_map::MemoryMap, println,
    proto::media::file::{File, FileAttribute, FileInfo, FileMode},
    table::{cfg, system_table_raw}, Status
};
use xmas_elf::{program::Type, ElfFile};

const PAGE_4KIB: usize = 0x1000;

macro_rules! arch {
    ($arch:literal, $modname:ident) => {
        #[cfg(target_arch = $arch)] mod $modname;
        #[cfg(target_arch = $arch)] use $modname as arch;
    };
}

arch!("aarch64", aarch64);
arch!("x86_64", amd64);

#[repr(C)]
pub struct RelaEntry {
    offset: u64,
    info: u64,
    addend: u64,
}

#[entry]
fn ignite() -> Status {
    let efi_ram_layout = memory_map(MemoryType::LOADER_DATA).unwrap();
    let descriptor_largest = efi_ram_layout.entries()
        .filter(|e| e.ty == MemoryType::CONVENTIONAL)
        .max_by_key(|e| e.page_count).unwrap();
    let ram_base = descriptor_largest.phys_start;

    let mut filesys_protocol = get_image_file_system(image_handle()).unwrap();
    let mut root = filesys_protocol.open_volume().unwrap();

    let mut file = root.open(
        cstr16!("\\bin\\unix-v11"), FileMode::Read, FileAttribute::empty()
    ).unwrap().into_regular_file().unwrap();

    let mut info_buf = [0u8; 512];
    let info = file.get_info::<FileInfo>(&mut info_buf).unwrap();
    let kernel_size = info.file_size() as usize;

    let pages = (kernel_size + PAGE_4KIB - 1) / PAGE_4KIB;
    let ptr = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages).unwrap();
    let kernel = unsafe { from_raw_parts_mut(ptr.as_ptr(), kernel_size) };
    file.read(kernel).unwrap();

    let elf = ElfFile::new(&kernel).unwrap();
    for ph in elf.program_iter() {
        match ph.get_type() {
            Ok(Type::Load) | Ok(Type::Dynamic) => {
                let mem_size = ph.mem_size() as usize;
                let phys_addr = (ram_base + ph.physical_addr()) as *mut u8;
                let offset = ph.offset() as usize;
                let file_size = ph.file_size() as usize;

                unsafe {
                    copy(kernel[offset..offset+file_size].as_ptr(), phys_addr, file_size);
                    write_bytes(phys_addr.add(file_size), 0, mem_size - file_size);
                }
            }
            _ => {}
        }
    }

    let rela_addr = elf.find_section_by_name(".rela.dyn").unwrap().address();
    let rela_size = elf.find_section_by_name(".rela.dyn").unwrap().size();
    let rela_ptr = (ram_base + rela_addr) as *mut RelaEntry;
    let entry_count = rela_size as usize / core::mem::size_of::<RelaEntry>();
    for i in 0..entry_count {
        let entry = unsafe { &*rela_ptr.add(i) };
        let typ = entry.info & 0xffffffff;
        if typ == 8 { // R_X86_64_RELATIVE
            let reloc_addr = (ram_base + entry.offset) as *mut u64;
            unsafe { *reloc_addr = ram_base + entry.addend; }
        }
    }

    let entrypoint = elf.header.pt2.entry_point() as usize + ram_base as usize;
    let spark: extern "efiapi" fn(Ember) -> ! = unsafe { core::mem::transmute(entrypoint) };
    let efi_ram_layout = unsafe { exit_boot_services(MemoryType::LOADER_DATA) };
    let stack_ptr = arch::stack_ptr();
    let ember = Ember {
        layout_ptr: efi_ram_layout.buffer().as_ptr() as *const RAMDescriptor,
        layout_len: efi_ram_layout.len(),
        kernel_size,
        stack_ptr
    };
    spark(ember);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("Panic: {}", info);
    loop { arch::halt(); }
}