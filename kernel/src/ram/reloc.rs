use crate::{
    arch::{R_RELATIVE, mmu::flags, move_stack},
    ram::{STACK_SIZE, glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}},
    sysinfo::{RAMType, RelaEntry}, SYS_INFO
};

use core::{mem::transmute, sync::atomic::{compiler_fence, Ordering}};

static mut SPARK_PTR: usize = 0;
static mut OLD_KBASE: usize = 0;

pub fn reloc() -> ! {
    let kinfo;
    let new_kbase;
    let jump_target;
    kinfo = SYS_INFO.lock().kernel;

    // Kernel allocation
    new_kbase = PHYS_ALLOC.alloc(
        AllocParams::new(kinfo.size).as_type(RAMType::Kernel)
    ).expect("Failed to allocate Hi-Half Kernel");

    // Stack allocation
    let stack_ptr = PHYS_ALLOC.alloc(
        AllocParams::new(STACK_SIZE).as_type(RAMType::KernelData)
    ).unwrap();
    SYS_INFO.set_new_stack_base(stack_ptr.addr() + stack_ptr.size());

    jump_target = !((1 << (GLACIER.cfg().va_bits - 1)) - 1);
    GLACIER.map_range(jump_target, new_kbase.addr(), kinfo.size, flags::K_RWO);
    GLACIER.map_range(jump_target + kinfo.text_ptr, new_kbase.addr() + kinfo.text_ptr, kinfo.text_len, flags::K_ROX);
    SYS_INFO.lock().kernel.base = new_kbase.addr();
    let old_kbase = kinfo.base;

    // KERNEL CLONE
    unsafe { (old_kbase as *const u8).copy_to(new_kbase.ptr(), kinfo.size); }
    // EVERY MODIFICATION OF STATIC VARIABLES ARE VOID BEYOND THIS POINT.

    let delta = jump_target - old_kbase;
    let rela = unsafe { core::slice::from_raw_parts((kinfo.rela_ptr + old_kbase) as *const RelaEntry, kinfo.rela_len) };

    // Relocation
    for entry in rela.iter() {
        let ty = entry.info & 0xffffffff;
        if ty == R_RELATIVE {
            let addr = (new_kbase.addr() + entry.offset as usize) as *mut u64;
            unsafe { *addr += delta as u64; }
        }
    }

    unsafe {
        (&raw mut SPARK_PTR).write_volatile(crate::spark as usize + delta);
        (&raw mut OLD_KBASE).write_volatile(old_kbase);
        compiler_fence(Ordering::Release);
        move_stack(&stack_ptr);
        transmute::<usize, extern "C" fn(usize) -> !>(SPARK_PTR)(OLD_KBASE);
    }
}
