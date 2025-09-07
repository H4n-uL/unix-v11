use crate::{
    arch::{mmu::flags, R_RELATIVE},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}},
    sysinfo::{ramtype, RelaEntry}, SYS_INFO
};

pub fn reloc() -> ! {
    let kinfo;
    let new_kbase;
    let jump_target;
    { // Mutex lock
        let glacier = GLACIER.lock();
        let mut sysinfo = SYS_INFO.lock();
        let mut phys_alloc = PHYS_ALLOC.lock();
        kinfo = sysinfo.kernel;

        new_kbase = phys_alloc.alloc(
            AllocParams::new(kinfo.size).as_type(ramtype::KERNEL)
        ).expect("Failed to allocate Hi-Half Kernel");

        jump_target = !((1 << (glacier.cfg().va_bits - 1)) - 1);
        glacier.map_range(
            jump_target, new_kbase.addr(), kinfo.size,
            flags::K_RWO, &mut phys_alloc
        );
        glacier.map_range(
            jump_target + kinfo.text_ptr, new_kbase.addr() + kinfo.text_ptr,
            kinfo.text_len, flags::K_ROX, &mut phys_alloc
        );
        sysinfo.kernel.base = new_kbase.addr();
    } // Mutex unlock
    let old_kbase = kinfo.base;

    unsafe { core::ptr::copy(old_kbase as *const u8, new_kbase.ptr(), kinfo.size); }

    let delta = jump_target - old_kbase;
    let rela = unsafe { core::slice::from_raw_parts((kinfo.rela_ptr + old_kbase) as *const RelaEntry, kinfo.rela_len) };

    for entry in rela.iter() {
        let ty = entry.info & 0xffffffff;
        if ty == R_RELATIVE {
            let addr = (new_kbase.addr() + entry.offset as usize) as *mut u64;
            unsafe { *addr += delta as u64; }
        }
    }

    let spark_ptr = crate::spark as usize + delta;
    let spark: fn(usize) -> ! = unsafe { core::mem::transmute(spark_ptr) };
    spark(old_kbase);
}