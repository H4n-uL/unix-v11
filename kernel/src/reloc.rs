use crate::{
    arch::{mmu::flags, move_stack, R_RELATIVE},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}, STACK_SIZE},
    sysinfo::{ramtype, RelaEntry}, SYS_INFO
};

pub fn reloc() -> ! {
    let kinfo;
    let new_kbase;
    let jump_target;
    kinfo = SYS_INFO.lock().kernel;

    // Kernel allocation
    new_kbase = PHYS_ALLOC.alloc(
        AllocParams::new(kinfo.size).as_type(ramtype::KERNEL)
    ).expect("Failed to allocate Hi-Half Kernel");

    // Stack allocation
    let stack_ptr = PHYS_ALLOC.alloc(
        AllocParams::new(STACK_SIZE).as_type(ramtype::KERNEL_DATA)
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

    let spark_ptr = crate::spark as usize + delta;
    let spark: extern "C" fn(usize) -> ! = unsafe { core::mem::transmute(spark_ptr) };
    unsafe { move_stack(&stack_ptr); }
    spark(old_kbase);
}
