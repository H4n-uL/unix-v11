use crate::{
    arch::{R_RELATIVE, move_stack, rvm::flags},
    kargs::{KBASE, KINFO, RAMType, RelaEntry, STACK_BASE},
    ram::{
        KHEAP, PAGE_4KIB, STACK_SIZE, align_up,
        glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}
    }
};

use core::{
    mem::transmute,
    sync::atomic::{AtomicUsize, Ordering as AtomOrd}
};

pub static SPARK_PTR: AtomicUsize = AtomicUsize::new(0);

pub fn reloc() -> ! {
    let kinfo = *KINFO.read();
    let new_kbase;
    let jump_target;

    // Kernel allocation
    new_kbase = PHYS_ALLOC.alloc(
        AllocParams::new(kinfo.size).as_type(RAMType::Kernel)
    ).expect("Failed to allocate Hi-Half Kernel");

    // Stack allocation
    let stack_ptr = PHYS_ALLOC.alloc(
        AllocParams::new(STACK_SIZE).as_type(RAMType::KernelData)
    ).unwrap();
    STACK_BASE.store(stack_ptr.addr() + stack_ptr.size(), AtomOrd::SeqCst);

    jump_target = !((1 << (GLACIER.read().cfg().va_bits - 1)) - 1);

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
        KHEAP.lock().oom_handler.set_base(align_up(jump_target + kinfo.size, PAGE_4KIB));

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
        move_stack(&stack_ptr);
        transmute::<usize, extern "C" fn() -> !>(
            SPARK_PTR.load(AtomOrd::SeqCst)
        )();
    }
}
