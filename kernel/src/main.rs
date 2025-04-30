//!          Uniplexed Information and Computing Service Version 11          !//
//!
//! Crafted by HaמuL in 2025
//! Description: Kernel of Research UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]
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

arch!("x86_64", amd64);
arch!("aarch64", aarch64);
arch!("riscv64", riscv64);

fn init_metal(ember: &Ember) {
    arch::init_serial();
    ram::init_ram();
    printk!("Uniplexed Information and Computing Service Version 11\n");
    device::init_device(ember);
}
fn exec_aleph() {}
fn schedule() -> ! { loop { arch::halt(); } }

pub static STACK_BASE: Mutex<usize> = Mutex::new(0);

#[no_mangle]
pub extern "efiapi" fn flare(mut ember: Ember) -> ! {
    ember.protect();
    RAM_BLOCK_MANAGER.lock().init(&mut ember);
    init_metal(&ember);
    exec_aleph();
    schedule();
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop { arch::halt(); }
}