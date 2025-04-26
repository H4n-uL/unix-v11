//!          Uniplexed Information and Computing Service Version 11          !//
//!
//! Crafted by HaמuL in 2025
//! Description: Kernel of Research UNIX Version 11
//! Licence: Public Domain

#![no_std]
#![no_main]
extern crate alloc;

mod device; mod ember; mod ram;
use core::panic::PanicInfo;
use ember::Ember;

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

arch!("aarch64", aarch64);
arch!("x86_64", amd64);

fn init_metal(ember: Ember) {
    arch::init_serial();
    let raminfo = ram::get_ram_info(ember.efi_ram_layout());
    ram::init_ram(raminfo, ember.kernel_size, ember.stack_ptr);
    device::init_device();
}
fn exec_aleph() {}
fn schedule() -> ! { loop { arch::halt(); } }

#[no_mangle]
pub extern "efiapi" fn flare(ember: Ember) -> ! {
    init_metal(ember);
    printk!("Uniplexed Information and Computing Service Version 11\n");
    exec_aleph();
    schedule();
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop { arch::halt(); }
}