pub mod glacier;
pub mod physalloc;
pub mod reloc;

use crate::{
    arch::mmu::flags,
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}},
    sysinfo::RAMType
};

use core::{alloc::Layout, ops::{Deref, DerefMut}};
use spin::Mutex;
use talc::{OomHandler, Talc, Talck};

pub const PAGE_4KIB: usize = 0x1000;
pub const STACK_SIZE: usize = 0x4000;
pub const HEAP_SIZE: usize = 0x100000;

static mut KHEAP_VLOC: usize = 0;

// For DMA or other physical page-aligned buffers
pub struct PhysPageBuf {
    ptr: *mut u8,
    size: usize
}

impl PhysPageBuf {
    pub fn new(size: usize) -> Self {
        let ptr = PHYS_ALLOC.alloc(
            AllocParams::new(size)
                .align(PAGE_4KIB)
                .as_type(RAMType::KernelData)
        ).expect("Failed to allocate page-aligned RAM");
        return Self { ptr: ptr.ptr(), size: ptr.size() };
    }
}

impl Drop for PhysPageBuf {
    fn drop(&mut self) {
        unsafe { PHYS_ALLOC.free_raw(self.ptr, self.size); }
    }
}

impl Deref for PhysPageBuf {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl DerefMut for PhysPageBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

struct KOoRAM;

impl OomHandler for KOoRAM {
    fn handle_oom(talc: &mut Talc<Self>, layout: Layout) -> Result<(), ()> {
        let ptr = PHYS_ALLOC.alloc(
            AllocParams::new(layout.size() * 2)
                .as_type(RAMType::KernelData)
                .align(PAGE_4KIB)
        ).ok_or(())?;

        unsafe {
            GLACIER.map_range(
                KHEAP_VLOC,
                ptr.addr(),
                ptr.size(),
                flags::K_RWO
            );
            let vheap = core::slice::from_raw_parts(
                KHEAP_VLOC as *const u8, ptr.size()
            );
            KHEAP_VLOC += ptr.size();
            talc.claim(vheap.into())?;
        }
        return Ok(());
    }
}

#[global_allocator]
static ALLOCATOR: Talck<Mutex<()>, KOoRAM> = Talc::new(KOoRAM).lock();

pub fn align_up(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return val.div_ceil(align) * align;
}

pub fn init_ram() {
    let available = PHYS_ALLOC.available();
    let heap_size = ((available as f64 * 0.05) as usize).max(HEAP_SIZE);
    let heap_ptr = PHYS_ALLOC.alloc(
        AllocParams::new(heap_size)
            .as_type(RAMType::KernelData)
            .align(PAGE_4KIB)
    ).unwrap();

    unsafe {
        GLACIER.map_range(
            KHEAP_VLOC,
            heap_ptr.addr(),
            heap_ptr.size(),
            flags::K_RWO
        );
        let vheap = core::slice::from_raw_parts(
            KHEAP_VLOC as *const u8, heap_ptr.size()
        );
        KHEAP_VLOC += heap_ptr.size();
        ALLOCATOR.lock().claim(vheap.into()).unwrap();
    }
}

pub fn dump_bytes(buf: &[u8]) {
    const LINE: usize = 16;
    let mut offset = 0;
    for line in buf.chunks(LINE) {
        crate::printk!("{:08x}  ", offset);
        for (i, byte) in line.iter().enumerate() {
            if i == LINE / 2 { crate::printk!(" "); }
            crate::printk!("{:02x} ", byte);
        }
        for i in line.len()..LINE {
            if i == LINE / 2 { crate::printk!(" "); }
            crate::printk!("   ");
        }
        crate::printk!("   |");
        for byte in line { crate::printk!("{}",
            if (0x20..0x7f).contains(byte) { *byte as char } else { '.' }
        ); }
        crate::printlnk!("|");
        offset += line.len();
    }
    crate::printlnk!("{:08x}", offset);
}
