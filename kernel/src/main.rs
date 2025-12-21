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
mod ram; mod sort; mod sysinfo;

use crate::{
    ram::{
        STACK_SIZE,
        glacier::GLACIER,
        physalloc::PHYS_ALLOC,
        reloc::OLD_KBASE
    },
    sysinfo::{RAMType, SYS_INFO, SysInfo}
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
pub extern "efiapi" fn ignite(sysinfo: SysInfo) -> ! {
    SYS_INFO.init(sysinfo);
    PHYS_ALLOC.init(SYS_INFO.efi_ram_layout_mut());
    GLACIER.init();

    arch::init_serial();
    ram::reloc::reloc();
}

#[unsafe(no_mangle)]
pub extern "C" fn spark() -> ! {
    unsafe {
        let ksize = SYS_INFO.lock().kernel.size;
        PHYS_ALLOC.free_raw(OLD_KBASE as *mut u8, ksize);
    }

    // arch::inter::init();
    printlnk!("The UNIX Time-Sharing System: Eleventh Edition");
    PHYS_ALLOC.reclaim();
    device::init_device();
    let _ = filesys::init_filesys();
    // exec_aleph();

    let stack_usage = SYS_INFO.lock().stack_base - crate::arch::stack_ptr() as usize;
    printlnk!("Kernel stack usage: {} / {} bytes", stack_usage, STACK_SIZE);

    let nonconv = PHYS_ALLOC.filtsize(|b| b.ty() != RAMType::Conv);
    let ram_used = PHYS_ALLOC.total() - PHYS_ALLOC.available() - nonconv;
    printlnk!("RAM used: {:.6} MB", ram_used as f64 / 1000000.0);
    printlnk!("Non-conventional RAM: {:.6} MB", nonconv as f64 / 1000000.0);

    loop { arch::halt(); }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    printlnk!("{}", info);
    loop { arch::halt(); }
}
