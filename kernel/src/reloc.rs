use crate::{
    arch::{mmu::flags, R_RELATIVE},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}},
    sysinfo::{ramtype, RelaEntry}, SYS_INFO
};

pub fn reloc() -> ! {
    let kinfo = SYS_INFO.lock().kernel;
    let old_kbase = kinfo.base;
    let ksize = kinfo.size;
    let rela_ptr = kinfo.rela_ptr;
    let rela_len = kinfo.rela_len;

    let high_half: usize = !((1 << (GLACIER.cfg().va_bits - 1)) - 1);
    let new_kbase = PHYS_ALLOC.alloc(
        AllocParams::new(ksize).as_type(ramtype::KERNEL)
    ).expect("Failed to allocate Hi-Half Kernel");

    GLACIER.map_range(high_half, new_kbase.addr(), ksize, flags::PAGE_DEFAULT);
    SYS_INFO.lock().kernel.base = new_kbase.addr();
    unsafe { core::ptr::copy(old_kbase as *const u8, new_kbase.ptr(), ksize); }

    let delta = high_half - old_kbase;
    let rela = unsafe { core::slice::from_raw_parts((rela_ptr + old_kbase) as *const RelaEntry, rela_len) };

    for entry in rela.iter() {
        let ty = entry.info & 0xffffffff;
        if ty == R_RELATIVE {
            let addr = (new_kbase.addr() + entry.offset as usize) as *mut u64;
            unsafe { *addr += delta as u64; }
        }
    }

    let spark_ptr = crate::spark as usize + delta;
    let spark = unsafe { core::mem::transmute::<usize, extern "C" fn(usize) -> !>(spark_ptr) };
    spark(old_kbase);
}