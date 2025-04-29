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

fn init_metal(ember: &mut Ember) {
    arch::init_serial();
    ram::init_ram(ember);
    printk!("Uniplexed Information and Computing Service Version 11\n");
    device::init_device(ember);
}
fn exec_aleph() {}
fn schedule() -> ! { loop { arch::halt(); } }

pub static STACK_BASE: Mutex<usize> = Mutex::new(0);

#[no_mangle]
pub extern "efiapi" fn flare(mut ember: Ember) -> ! {
    *STACK_BASE.lock() = ember.stack_base;
    ember.protect();
    ember.sort_ram_layout();
    init_metal(&mut ember);
    let ramblock = RAM_BLOCK_MANAGER.lock();
    for desc in ember.efi_ram_layout() { printk!("{:?}\n", desc); } printk!("\n");
    for block in ramblock.blocks() { printk!("{:?}\n", block); } printk!("\n");
    printk!("Kernel base: {}\n", ember.kernel_base);
    exec_aleph();
    schedule();
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop { arch::halt(); }
}