use crate::{
    arch::{R_RELATIVE, move_stack, rvm::flags},
    kargs::{AP_VID, KBASE, KINFO, RAMType, RelaEntry},
    ram::{
        GLEAM_BASE, PER_CPU_DATA, STACK_SIZE,
        glacier::{GLACIER, HIHALF},
        physalloc::{AllocParams, PHYS_ALLOC}
    }
};

use core::{
    mem::transmute,
    sync::atomic::{AtomicUsize, Ordering as AtomOrd}
};

pub static SPARK_PTR: AtomicUsize = AtomicUsize::new(0);

pub fn reloc() -> ! {
    let kinfo = *KINFO.read();
    let jump_target = HIHALF.load(AtomOrd::Relaxed);

    // Kernel allocation
    let new_kbase = PHYS_ALLOC.alloc(
        AllocParams::new(kinfo.size).as_type(RAMType::Kernel)
    ).expect("Failed to allocate Hi-Half Kernel");

    // Stack allocation
    let stack_ptr = PHYS_ALLOC.alloc(
        AllocParams::new(STACK_SIZE).as_type(RAMType::KernelData)
    ).unwrap();

    // Per-CPU stack mapping
    let stack_va = GLEAM_BASE - (PER_CPU_DATA * AP_VID.assign()) - STACK_SIZE;
    GLACIER.write().map_range(
        stack_va, stack_ptr.addr(),
        STACK_SIZE, flags::K_RWO
    );

    // Kernel mapping
    GLACIER.write().map_range(
        jump_target, new_kbase.addr(),
        kinfo.size, flags::K_RWO
    );
    GLACIER.write().map_range(
        jump_target + kinfo.text_ptr, new_kbase.addr() + kinfo.text_ptr,
        kinfo.text_len, flags::K_ROX
    );

    // Kernel base update as physical address
    let old_kbase = KBASE.load(AtomOrd::SeqCst);
    KBASE.store(new_kbase.addr(), AtomOrd::SeqCst);
    let delta = jump_target - old_kbase;

    // KERNEL CLONE
    unsafe {
        SPARK_PTR.store(crate::spark as *const () as usize + delta, AtomOrd::SeqCst);
        (old_kbase as *const u8).copy_to(new_kbase.ptr(), kinfo.size);
    }
    // ANY MODIFICATION OF STATIC VARIABLES IS VOID BEYOND THIS POINT.

    let rela = unsafe {
        core::slice::from_raw_parts(
            (kinfo.rela_ptr + old_kbase) as *const RelaEntry,
            kinfo.rela_len
        )
    };

    // Relocation
    for entry in rela.iter() {
        let ty = entry.info & 0xffffffff;
        if ty == R_RELATIVE {
            let addr = (new_kbase.addr() + entry.offset) as *mut usize;
            unsafe { *addr += delta; }
        }
    }

    // JUMP
    unsafe {
        // ALL STACK VARIABLES ARE VOID BEYOND THIS POINT.
        move_stack(stack_va, stack_ptr.size());
        transmute::<usize, extern "C" fn() -> !>(
            SPARK_PTR.load(AtomOrd::SeqCst)
        )();
    }
}
