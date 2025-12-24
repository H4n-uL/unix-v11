//!                       The UNIX Time-Sharing System                       !//
//!                             Eleventh Edition                             !//
//!
//! Crafted by HaƞuL in 2025
//! Description: Kernel of UNIX Version 11
//! Licence: Non-assertion

#![no_std]
#![no_main]

extern crate alloc;

mod arch; mod device; mod filesys;
mod kargs; mod ram; mod sort;

use crate::{
    kargs::{KINFO, Kargs, RAMType, STACK_BASE, set_kargs},
    ram::{
        STACK_SIZE,
        glacier::init_glacier,
        physalloc::PHYS_ALLOC,
        reloc::OLD_KBASE
    }
};

use core::{panic::PanicInfo, sync::atomic::Ordering as AtomOrd};

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
    set_kargs(kargs);
    // KARGS.init(kargs);
    PHYS_ALLOC.init();
    init_glacier();

    arch::init_serial();
    ram::reloc::reloc();
}

#[unsafe(no_mangle)]
pub extern "C" fn spark() -> ! {
    unsafe {
        let ksize = KINFO.read().size;
        PHYS_ALLOC.free_raw(OLD_KBASE as *mut u8, ksize);
    }

    // arch::inter::init();
    printlnk!("The UNIX Time-Sharing System: Eleventh Edition");
    PHYS_ALLOC.reclaim();
    device::init_device();
    let _ = filesys::init_filesys();
    // exec_aleph();

    let stack_usage = STACK_BASE.load(AtomOrd::SeqCst) - crate::arch::stack_ptr() as usize;
    printlnk!("Kernel stack usage: {} / {} bytes", stack_usage, STACK_SIZE);

    let ram_used = PHYS_ALLOC.total() - PHYS_ALLOC.available();
    printlnk!("RAM used: {:.6} MB", ram_used as f64 / 1000000.0);

    let ksize = PHYS_ALLOC.filtsize(|b| b.ty() == RAMType::Kernel);
    printlnk!("Loaded kimg size: {:.3} kB", ksize as f64 / 1000.0);

    loop { arch::halt(); }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    printlnk!("{}", info);
    loop { arch::halt(); }
}
