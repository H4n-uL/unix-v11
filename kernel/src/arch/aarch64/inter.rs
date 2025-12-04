use core::arch::asm;

pub fn get() -> bool {
    let daif: u64;
    unsafe {
        asm!("mrs {}, daif", out(reg) daif, options(nomem, nostack, preserves_flags));
    }
    return (daif & 0b1111) != 0;
}

pub fn set(enabled: bool) {
    unsafe {
        if enabled {
            asm!("msr daifset, #0b1111", options(nomem, nostack, preserves_flags));
        } else {
            asm!("msr daifclr, #0b1111", options(nomem, nostack, preserves_flags));
        }
    }
}
