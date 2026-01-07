use crate::{
    kargs::ap_vid, printlnk,
    proc::PROCS, ram::glacier::GLACIER
};

use core::slice::from_raw_parts;

#[unsafe(no_mangle)]
pub extern "C" fn kernel_requestee(
    req: *const u8,
    arg1: usize, arg2: usize, arg3: usize,
    arg4: usize, arg5: usize, arg6: usize
) -> usize {
    let len = (0..16)
        .find(|&i| unsafe { *req.add(i) } == 0)
        .unwrap_or(16);

    match unsafe { from_raw_parts(req, len) } {
        b"_print" => {
            for i in 0..arg2 {
                crate::arch::serial_putchar(
                    unsafe { *(arg1 as *const u8).add(i) }
                );
            }
        }
        b"exit" => {
            GLACIER.read().activate();
            PROCS.write().running.remove(&ap_vid());
            printlnk!("exit code: {}", arg1);
            loop { crate::arch::halt(); }
        }
        // ... kernel request impls goes here ...
        _ => {}
    }

    return 0;
}
