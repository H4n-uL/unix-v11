//!          Uniplexed Information and Computing Service Version 11          !//
//!
//! Crafted by HaמuL in 2025
//! Description: Kernel of Research UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt, abi_riscv_interrupt)]
extern crate alloc;

mod device; mod ember;
mod ram; mod ramblock;
mod sort;

use core::panic::PanicInfo;
use ember::Ember;
use ramblock::RAM_BLOCK_MANAGER;
use spin::Mutex;

macro_rules! arch {
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
    () => { $crate::printk!("\n"); };
    ($($arg:tt)*) => { $crate::printk!("{}\n", format_args!($($arg)*)) };
}

arch!("x86_64", amd64);
arch!("aarch64", aarch64);
arch!("riscv64", riscv64);

fn init_metal() {
    arch::init_exceptions();
    arch::init_serial();
    ram::init_ram();
    printlnk!("Uniplexed Information and Computing Service Version 11");
    device::init_device();
}
fn exec_aleph() {}
fn schedule() -> ! { loop { arch::halt(); } }

pub static EMBER: Mutex<Ember> = Mutex::new(Ember::empty());

#[no_mangle]
pub extern "efiapi" fn flame(ember: Ember) -> ! {
    EMBER.lock().init(ember);
    RAM_BLOCK_MANAGER.lock().init();
    init_metal();
    exec_aleph();
    schedule();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    printlnk!("{}", info);
    loop { arch::halt(); }
}