#![no_std]
#![no_main]

use core::panic::PanicInfo;

fn kernel_request(
    req: *const u8,
    arg1: usize, arg2: usize, arg3: usize,
    arg4: usize, arg5: usize, arg6: usize
) -> usize {
    let ret;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "syscall",
            inlateout("rax") req => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            in("r9") arg6,
            out("rcx") _,
            out("r11") _
        );
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(
            "svc #0",
            inlateout("x0") req => ret,
            in("x1") arg1,
            in("x2") arg2,
            in("x3") arg3,
            in("x4") arg4,
            in("x5") arg5,
            in("x6") arg6
        );
    }
    return ret;
}

fn print(s: &str) {
    let bytes = s.as_bytes();
    kernel_request(
        b"_print\0".as_ptr(),
        bytes.as_ptr() as usize,
        bytes.len(),
        0, 0, 0, 0
    );
}

fn exit(code: u8) -> ! {
    let exit = b"exit\0";
    kernel_request(exit.as_ptr(), code as usize, 0, 0, 0, 0, 0);
    unreachable!();
}

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    print("Message from userland: It works!\n");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
