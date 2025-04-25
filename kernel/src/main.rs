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
use ram::{RAMDescriptor, PAGE_4KIB};

macro_rules! arch {
    ($arch:literal, $modname:ident) => {
        #[cfg(target_arch = $arch)] mod $modname;
        #[cfg(target_arch = $arch)] use $modname as arch;
    };
}

macro_rules! printk {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = core::write!($crate::arch::SerialWriter, $($arg)*);
    }};
}

arch!("aarch64", aarch64);
arch!("x86_64", amd64);

fn init_metal(efi_ram_layout: &[RAMDescriptor], kernel_size: usize) {
    arch::init_serial();
    let raminfo = ram::get_ram_info(efi_ram_layout);
    ram::init_ram(raminfo, kernel_size);
    // init_storage();
}
fn exec_aleph() {}
fn schedule() -> ! { loop { arch::halt(); } }

#[no_mangle]
pub extern "win64" fn ignite(layout_ptr: *const RAMDescriptor, layout_len: usize, kernel_size: usize) -> ! {
    let efi_ram_layout = unsafe { core::slice::from_raw_parts(layout_ptr, layout_len) };
    let kernel_size = ram::align_up(kernel_size, PAGE_4KIB);
    init_metal(efi_ram_layout, kernel_size);
    printk!("Uniplexed Information and Computing Service Version 11\n");
    printk!("\n");
    printk!("printk test: {}\n", 1234);
    let stack_variable = 0xfeedfacecafebabe as u64;
    printk!("Stack test variable: {:#x}\n", stack_variable);
    let heap_variable = alloc::boxed::Box::new(0xfeedfacecafebabe as u64);
    printk!("Heap test variable: {:#x}\n", *heap_variable.as_ref());

    exec_aleph();
    schedule();
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop { arch::halt(); }
}