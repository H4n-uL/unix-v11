use crate::{
    arch::{mmu::flags, R_RELATIVE},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}},
    sysinfo::{ramtype, RelaEntry}, SYS_INFO
};

#[inline(always)]
pub fn reloc() -> ! {
    let sysinfo = SYS_INFO.lock();
    let kbase = sysinfo.kernel.base;
    let ksize = sysinfo.kernel.size;
    let rela_ptr = sysinfo.kernel.rela_ptr;
    let rela_len = sysinfo.kernel.rela_len;
    drop(sysinfo);

    let high_half: usize = !((1 << (GLACIER.cfg().va_bits - 1)) - 1);
    let new_kbase = PHYS_ALLOC.alloc(
        AllocParams::new(ksize).as_type(ramtype::KERNEL)
    ).expect("Failed to allocate Hi-Half Kernel");

    GLACIER.map_range(high_half, new_kbase.addr(), ksize, flags::PAGE_DEFAULT);
    unsafe { PHYS_ALLOC.free_raw(kbase as *mut u8, ksize); }
    SYS_INFO.lock().kernel.base = new_kbase.addr();
    unsafe { core::ptr::copy(kbase as *const u8, new_kbase.ptr(), ksize); }

    let delta = high_half - kbase;
    let rela = unsafe { core::slice::from_raw_parts((rela_ptr + kbase) as *const RelaEntry, rela_len) };

    for entry in rela.iter() {
        let ty = entry.info & 0xffffffff;
        if ty == R_RELATIVE {
            let addr = (new_kbase.addr() + entry.offset as usize) as *mut u64;
            unsafe { *addr += delta as u64; }
        }
    }

    let spark_ptr = crate::spark as usize + delta;
    let spark = unsafe { core::mem::transmute::<usize, extern "C" fn() -> !>(spark_ptr) };
    spark();
}