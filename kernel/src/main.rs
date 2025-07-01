//!          Uniplexed Information and Computing Service Version 11          !//
//!
//! Crafted by HaמuL in 2025
//! Description: Kernel of Research UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(abi_riscv_interrupt)]

extern crate alloc;

mod device;
mod ram; mod sort; mod sysinfo;

use crate::{ram::physalloc::PHYS_ALLOC, sysinfo::SysInfo};
use core::panic::PanicInfo;
use spin::Mutex;

macro_rules! use_arch {
    ($arch:literal, $modname:ident) => {
        #[cfg(target_arch = $arch)] mod $modname;
        #[cfg(target_arch = $arch)] use $modname as arch;
    };
}

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

use_arch!("x86_64", amd64);
use_arch!("aarch64", aarch64);
use_arch!("riscv64", riscv64);

fn init_metal() {
    arch::exceptions::init();
    arch::init_serial();
    printlnk!("Uniplexed Information and Computing Service Version 11");
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
    init_metal();
    exec_aleph();
    schedule();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    printlnk!("{}", info);
    loop { arch::halt(); }
}