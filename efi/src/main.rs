//!                              EFI Bootloader                              !//
//!
//! Crafted by HaƞuL in 2025-2026
//! Description: EFI Bootloader of UNIX Version 11
//! Licence: Non-assertion pledge

#![no_std]
#![no_main]

mod arch;
mod kargs;

use crate::{arch::*, kargs::*};

use core::panic::PanicInfo;
use uefi::{
    Identify, Status,
    boot::{
        AllocateType, MemoryType, SearchType,
        allocate_pages, exit_boot_services,
        free_pages, get_image_file_system,
        image_handle, locate_handle_buffer,
        open_protocol_exclusive as open_protocol
    },
    cstr16, entry,
    mem::memory_map::MemoryMap,
    println,
    proto::media::{
        block::BlockIO,
        file::{File, FileAttribute, FileInfo, FileMode}
    },
    system::with_config_table,
    table::cfg::ConfigTableEntry
};
use xmas_elf::{program::Type as PhType, ElfFile};

const DT_NULL: usize   = 0;
const DT_STRTAB: usize = 5;
const DT_SYMTAB: usize = 6;
const DT_RELA: usize   = 7;
const DT_RELASZ: usize = 8;

const PAGE_4KIB: usize = 0x1000;

pub fn align_up(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return val.div_ceil(align) * align;
}

#[entry]
fn flint() -> Status {
    let mut file_binary: &mut [u8] = &mut [];
    if let Ok(mut filesys_protocol) = get_image_file_system(image_handle()) {
        let mut root = filesys_protocol.open_volume().unwrap();

        let mut file = root.open(
            cstr16!("\\unix"), FileMode::Read, FileAttribute::empty()
        ).unwrap().into_regular_file().unwrap();

        let mut info_buf = [0u8; 512];
        let info = file.get_info::<FileInfo>(&mut info_buf).unwrap();
        let file_size = info.file_size() as usize;

        let file_pages = align_up(file_size, PAGE_4KIB) / PAGE_4KIB;
        let file_ptr = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, file_pages).unwrap();
        file_binary = unsafe { core::slice::from_raw_parts_mut(file_ptr.as_ptr(), file_size) };
        file.read(file_binary).unwrap();
    }

    let elf = ElfFile::new(file_binary).unwrap();
    let ep = elf.header.pt2.entry_point() as usize;

    let ksize = elf.program_iter()
        .filter(|ph| ph.get_type() == Ok(PhType::Load))
        .map(|ph| ph.virtual_addr() + ph.mem_size())
        .max().unwrap() as usize;

    let kernel_pages = align_up(ksize, PAGE_4KIB) / PAGE_4KIB;
    let kbase = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_CODE, kernel_pages).unwrap().as_ptr() as usize;

    let seg_ptr = allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).unwrap().as_ptr() as usize;
    let mut seg_len = 0;

    for ph in elf.program_iter() {
        if let Ok(PhType::Load) = ph.get_type() {
            let offset = ph.offset() as usize;
            let file_size = ph.file_size() as usize;
            let mem_size = ph.mem_size() as usize;
            let virt_addr = ph.virtual_addr() as usize;
            let phys_addr = (kbase + virt_addr) as *mut u8;

            let seg = Segment {
                ptr: virt_addr,
                len: mem_size,
                flags: ph.flags().0,
                align: ph.align() as u32
            };

            unsafe {
                phys_addr.write_bytes(0, mem_size);
                file_binary[offset..][..file_size].as_ptr().copy_to(phys_addr, file_size);

                (seg_ptr as *mut Segment).add(seg_len).write(seg);
            }
            seg_len += 1;
        }
    }

    let (mut dyn_ptr, mut dyn_len) = (0, 0);
    for ph in elf.program_iter() {
        if ph.get_type() == Ok(PhType::Dynamic) {
            dyn_ptr = ph.virtual_addr() as usize;
            dyn_len = ph.mem_size() as usize / size_of::<DynEntry>();
            break;
        }
    }

    let dynamic = unsafe { core::slice::from_raw_parts((dyn_ptr + kbase) as *const DynEntry, dyn_len) };
    let (mut rela_ptr, mut rela_sz, mut symtab, mut strtab) = (0, 0, 0, 0);
    for entry in dynamic.iter() {
        *match entry.tag {
            DT_NULL => break,
            DT_RELA =>   &mut rela_ptr,
            DT_RELASZ => &mut rela_sz,
            DT_SYMTAB => &mut symtab,
            DT_STRTAB => &mut strtab,
            _ => continue
        } = entry.val;
    }

    let rela_len = rela_sz / size_of::<RelaEntry>();
    let rela = unsafe { core::slice::from_raw_parts((rela_ptr + kbase) as *const RelaEntry, rela_len) };
    let symbols = (symtab + kbase) as *const SymEntry;

    for entry in rela.iter() {
        let ty = entry.info & 0xffffffff;
        let sym_idx = entry.info >> 32;
        let reloc_addr = (kbase + entry.offset) as *mut usize;

        match ty {
            R_REL => {
                unsafe { *reloc_addr = kbase.wrapping_add_signed(entry.addend); }
            }
            _ if R_SYM.contains(&ty) => {
                let sym = unsafe { &*symbols.add(sym_idx) };
                let sym_addr = kbase + sym.value;
                unsafe { *reloc_addr = sym_addr.wrapping_add_signed(entry.addend); }
            }
            _ => {}
        }
    }

    let (acpi_ptr, dtb_ptr) = with_config_table(|config| {
        let (mut acpi_ptr, mut dtb_ptr) = (0, 0);
        for cfg in config.iter() {
            let isacpi = cfg.guid == ConfigTableEntry::ACPI_GUID && acpi_ptr == 0;
            let isacpi2 = cfg.guid == ConfigTableEntry::ACPI2_GUID;
            let isdtb = cfg.guid == ConfigTableEntry::SMBIOS3_GUID;
            if isacpi && acpi_ptr == 0 || isacpi2 {
                acpi_ptr = cfg.address as usize;
            }
            if isdtb {
                dtb_ptr  = cfg.address as usize;
            }
        }

        return (acpi_ptr, dtb_ptr);
    });

    let mut disk_uuid = [0u8; 16];
    if let Ok(handle_buffer) = locate_handle_buffer(SearchType::ByProtocol(&BlockIO::GUID)) {
        for &handle in handle_buffer.iter() {
            if let Ok(block_io) = open_protocol::<BlockIO>(handle) {
                let media = block_io.media();
                if media.is_logical_partition() { continue; }

                let block_size = media.block_size() as usize;
                let gpt_header_pages = align_up(block_size, PAGE_4KIB) / PAGE_4KIB;
                let gpt_header_ptr = allocate_pages(
                    AllocateType::AnyPages,
                    MemoryType::LOADER_DATA,
                    gpt_header_pages
                ).unwrap();

                let gpt_header = unsafe { core::slice::from_raw_parts_mut(gpt_header_ptr.as_ptr(), block_size) };
                if block_io.read_blocks(media.media_id(), 1, gpt_header).is_ok() && &gpt_header[0..8] == b"EFI PART" {
                    disk_uuid.copy_from_slice(&gpt_header[56..72]);
                    break;
                }
                unsafe { let _ = free_pages(gpt_header_ptr, gpt_header_pages); }
            }
        }
    }

    let ignite: extern "efiapi" fn(Kargs) -> ! = unsafe { core::mem::transmute(ep + kbase) };
    let efi_ram_layout = unsafe { exit_boot_services(Some(MemoryType::LOADER_DATA)) };
    let sysinfo = Kargs {
        kernel: KernelInfo {
            size: ksize, ep,
            seg_ptr, seg_len,
            dyn_ptr, dyn_len
        },
        sys: SysInfo {
            layout_ptr: efi_ram_layout.buffer().as_ptr() as usize,
            layout_len: efi_ram_layout.len(),
            acpi_ptr, dtb_ptr, disk_uuid
        },
        kbase
    };
    ignite(sysinfo);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("Panic: {}", info);
    loop { arch::halt(); }
}
