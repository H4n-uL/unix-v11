//!          Uniplexed Information and Computing Service Version 11          !//
//!
//! Crafted by HaמuL in 2025
//! Description: Kernel of Research UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]
extern crate alloc;

mod ram;
use core::panic::PanicInfo;
use ram::RAMDescriptor;

macro_rules! arch {
    ($arch:literal, $modname:ident) => {
        #[cfg(target_arch = $arch)] mod $modname;
        #[cfg(target_arch = $arch)] use $modname as arch;
    };
}

arch!("aarch64", aarch64);
arch!("x86_64", amd64);

fn init_metal(efi_ram_layout: &[RAMDescriptor]) {
    ram::init_ram(efi_ram_layout);
    // init_storage();
}
fn exec_aleph() {}
fn schedule() -> ! { loop { arch::halt(); } }

#[no_mangle]
pub extern "win64" fn ignite(layout_ptr: *const RAMDescriptor, layout_len: usize) -> ! {
    let efi_ram_layout = unsafe { core::slice::from_raw_parts(layout_ptr, layout_len) };
    arch::init_serial();
    arch::serial_print("Research UNIX Version 11\n");
    init_metal(efi_ram_layout);
    exec_aleph();

    let heap_variable = alloc::boxed::Box::new(0xfeedfacecafebabe as u64);
    arch::serial_print("Heap variable: ");
    arch::print_u64(*heap_variable.as_ref());
    arch::serial_print("\n");

    schedule();
}

extern "C" {
    static _kernel_start: u8;
    static _kernel_end: u8;
}

pub fn kernel_size() -> usize {
    // let start = unsafe { &_kernel_start as *const u8 as usize };
    // let end = unsafe { &_kernel_end as *const u8 as usize };
    // let size = end - start;
    let size = 0x200000; // 2MB, temporary

    return size;
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop { arch::halt(); }
}