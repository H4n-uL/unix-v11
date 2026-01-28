use crate::{arch, proc::exit_proc, ram::glacier::hihalf};

use core::slice::from_raw_parts;

macro_rules! check_fault {
    ($ptr:tt, $ctr:tt, $sz:ty) => { {
        const INVALID_VA: usize = 1 << (usize::BITS - 1);
        let ctr_end = $ptr.saturating_add(
            $ctr.saturating_mul(size_of::<$sz>())
        );

        if ctr_end >= hihalf() {
            let _ = unsafe { (INVALID_VA as *const u8).read_volatile() };
        }
    } };
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_requestee(
    req: *const u8,
    arg1: usize, arg2: usize, arg3: usize,
    arg4: usize, arg5: usize, arg6: usize
) -> usize {
    let len = (0..16)
        .find(|&i| unsafe { *req.add(i) } == 0)
        .unwrap_or(16);

    let req = unsafe { from_raw_parts(req, len) };

    if req == b"exit" {
        exit_proc(arg1 as i32);
    }

    match req {
        b"open" => {
            let path = unsafe {
                let mut len = 0usize;
                while *(arg1 as *const u8).add(len) != 0 {
                    len += 1;
                }
                from_raw_parts(arg1 as *const u8, len)
            };
            check_fault!(arg1, (path.len() + 1), u8);
        }
        b"_print" => { // This syscall is for debugging purposes only
            check_fault!(arg1, arg2, u8);
            for i in 0..arg2 {
                arch::serial_putchar(
                    unsafe { *(arg1 as *const u8).add(i) }
                );
            }
        }
        // ... kernel request impls goes here ...
        _ => {}
    }

    return 0;
}
