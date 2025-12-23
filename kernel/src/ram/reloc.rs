use crate::{
    arch::{R_RELATIVE, rvm::flags, move_stack},
    ram::{
        KHEAP, PAGE_4KIB, STACK_SIZE, align_up,
        glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}
    },
    sysinfo::{RAMType, RelaEntry, SYS_INFO}
};

use core::mem::transmute;

pub static SPARK_PTR: usize = 0;
pub static OLD_KBASE: usize = 0;

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

    // Kernel mapping
    GLACIER.map_range(
        jump_target, new_kbase.addr(),
        kinfo.size, flags::K_RWO
    );
    GLACIER.map_range(
        jump_target + kinfo.text_ptr, new_kbase.addr() + kinfo.text_ptr,
        kinfo.text_len, flags::K_ROX
    );


    // Kernel base update as physical address
    SYS_INFO.lock().kernel.base = new_kbase.addr();
    let old_kbase = kinfo.base;
    let delta = jump_target - old_kbase;

    // KERNEL CLONE
    unsafe {
        (&raw const SPARK_PTR as *mut usize).write_volatile(crate::spark as *const () as usize + delta);
        (&raw const OLD_KBASE as *mut usize).write_volatile(old_kbase);
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
            let addr = (new_kbase.addr() + entry.offset as usize) as *mut u64;
            unsafe { *addr += delta as u64; }
        }
    }

    // JUMP
    unsafe {
        // ALL STACK VARIABLES ARE VOID BEYOND THIS POINT.
        move_stack(&stack_ptr);
        transmute::<usize, extern "C" fn() -> !>(
            (&raw const SPARK_PTR).read_volatile()
        )();
    }
}
