//!                       The UNIX Time-Sharing System                       !//
//!                             Eleventh Edition                             !//
//!
//! Crafted by HaמuL in 2025
//! Description: Kernel of UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]

extern crate alloc;

mod arch; mod device;
mod ram; mod sort; mod sysinfo;

use crate::{
    ram::{glacier::GLACIER, physalloc::PHYS_ALLOC},
    sysinfo::SysInfo
};
use core::panic::PanicInfo;
use spin::Mutex;

#[macro_export]
macro_rules! printk {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = core::write!($crate::arch::SerialWriter, $($arg)*);
    }};
}

#[macro_export]
macro_rules! printlnk {
    () => { $crate::printk!("\r\n"); };
    ($($arg:tt)*) => { $crate::printk!("{}\r\n", format_args!($($arg)*)) };
}

fn init_metal() {
    // arch::exceptions::init();
    arch::init_serial();
    printlnk!("The UNIX Time-Sharing System, Eleventh Edition");
    ram::init_ram();
    device::init_device();
}
fn exec_aleph() {}
fn schedule() -> ! { loop { arch::halt(); } }

pub static SYS_INFO: Mutex<SysInfo> = Mutex::new(SysInfo::empty());

#[unsafe(no_mangle)]
pub extern "efiapi" fn ignite(sysinfo: SysInfo) -> ! {
    SYS_INFO.lock().init(sysinfo);
    PHYS_ALLOC.init(SYS_INFO.lock().efi_ram_layout_mut());
    GLACIER.init();
    init_metal();
    exec_aleph();
    schedule();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    printlnk!("{}", info);
    loop { arch::halt(); }
}