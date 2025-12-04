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
    ram::{glacier::GLACIER, physalloc::PHYS_ALLOC},
    sysinfo::{SysInfo, SYS_INFO}
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
pub extern "C" fn spark(old_kbase: usize) -> ! {
    unsafe {
        let ksize = SYS_INFO.lock().kernel.size;
        PHYS_ALLOC.free_raw(old_kbase as *mut u8, ksize);
    }

    // arch::inter::init();
    printlnk!("The UNIX Time-Sharing System: Eleventh Edition");
    device::init_device();
    let _ = filesys::init_filesys();
    // exec_aleph();

    let stack_usage = crate::SYS_INFO.lock().stack_base - crate::arch::stack_ptr() as usize;
    printlnk!("Kernel stack usage: {} / 16384 bytes", stack_usage);
    let ram_used = crate::PHYS_ALLOC.total() - crate::PHYS_ALLOC.available();
    printlnk!("RAM used: {:.3} MiB", ram_used as f64 / 1048576.0);

    loop { arch::halt(); }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    printlnk!("{}", info);
    loop { arch::halt(); }
}
