use crate::{
    kargs::AP_LIST, printlnk, proc::PROCS,
    ram::glacier::{GLACIER, hihalf}
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
        b"_print" => { // This syscall is for debugging purposes only
            for i in 0..arg2 {
                if arg1 >= hihalf() {
                    break;
                    // This should cause a page fault
                    // before implementing page fault handler,
                    // all we can do is to just stop printing
                }
                crate::arch::serial_putchar(
                    unsafe { *(arg1 as *const u8).add(i) }
                );
            }
        }
        b"exit" => {
            GLACIER.read().activate();
            PROCS.write().running.remove(&AP_LIST.virtid_self());
            printlnk!("exit code: {}", arg1);
            loop { crate::arch::halt(); }
        }
        // ... kernel request impls goes here ...
        _ => {}
    }

    return 0;
}
