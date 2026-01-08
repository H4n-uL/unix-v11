//!                       The UNIX Time-Sharing System                       !//
//!                             Eleventh Edition                             !//
//!
//! Crafted by HaƞuL in 2025-2026
//! Description: Kernel of UNIX Version 11
//! Licence: Non-assertion pledge

#![no_std]
#![no_main]

extern crate alloc;

mod arch; mod device; mod filesys; mod kargs;
mod kreq; mod proc; mod ram; mod sort;

use crate::{
    kargs::{Kargs, RAMType, AP_LIST},
    ram::{
        STACK_SIZE,
        physalloc::PHYS_ALLOC,
        stack_top
    }
};

use core::panic::PanicInfo;

#[macro_export]
macro_rules! printk {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = core::write!($crate::arch::SerialWriter, $($arg)*);
    }};
}

#[macro_export]
macro_rules! printlnk {
    () => { $crate::printk!("\n"); };
    ($($arg:tt)*) => { $crate::printk!("{}\n", format_args!($($arg)*)) };
}

#[unsafe(no_mangle)]
pub extern "efiapi" fn ignite(kargs: Kargs) -> ! {
    kargs::set_kargs(kargs);
    // KARGS.init(kargs);
    PHYS_ALLOC.init();
    ram::glacier::init();
    ram::init_heap();

    arch::init_serial();
    ram::reloc::reloc();
}

#[unsafe(no_mangle)]
pub extern "C" fn spark() -> ! {
    arch::inter::init();
    printlnk!("The UNIX Time-Sharing System: Eleventh Edition");
    PHYS_ALLOC.reclaim();
    device::init_device();
    let _ = filesys::init_filesys();

    let stack_usage = stack_top() - crate::arch::stack_ptr() as usize;
    printlnk!("Kernel stack usage: {} / {} bytes", stack_usage, STACK_SIZE);

    printlnk!("ID of this AP: {}", AP_LIST.virtid_self());

    let ram_used = PHYS_ALLOC.filtsize(|b| b.used());
    printlnk!("RAM used: {:.6} MB", ram_used as f64 / 1000000.0);

    let ram_consumed = PHYS_ALLOC.filtsize(|b| b.ty() != RAMType::Conv || b.used());
    printlnk!("RAM consumed: {:.6} MB", ram_consumed as f64 / 1000000.0);

    let ksize = PHYS_ALLOC.filtsize(|b| b.ty() == RAMType::Kernel);
    printlnk!("Loaded kimg size: {:.3} kB", ksize as f64 / 1000.0);

    proc::exec_aleph();

    loop { arch::halt(); }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    printlnk!("{}", info);
    loop { arch::halt(); }
}
