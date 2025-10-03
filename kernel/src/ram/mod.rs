pub mod glacier;
pub mod physalloc;

use crate::{
    ram::physalloc::{AllocParams, PHYS_ALLOC},
    sysinfo::ramtype
};

use core::{alloc::Layout, ops::{Deref, DerefMut}};
use spin::Mutex;
use talc::{OomHandler, Talc, Talck};

pub const PAGE_4KIB: usize = 0x1000;
pub const STACK_SIZE: usize = 0x4000;
pub const HEAP_SIZE: usize = 0x100000;

pub struct PageAligned {
    ptr: *mut u8,
    size: usize
}

impl PageAligned {
    pub fn new(size: usize) -> Self {
        let ptr = PHYS_ALLOC.alloc(AllocParams::new(size))
            .expect("Failed to allocate page-aligned RAM");
        return Self { ptr: ptr.ptr(), size: ptr.size() };
    }
}

impl Drop for PageAligned {
    fn drop(&mut self) {
        unsafe { PHYS_ALLOC.free_raw(self.ptr, self.size); }
    }
}

impl Deref for PageAligned {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl DerefMut for PageAligned {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

struct KOoRAM;

impl OomHandler for KOoRAM {
    fn handle_oom(talc: &mut Talc<Self>, layout: Layout) -> Result<(), ()> {
        let ptr = PHYS_ALLOC.alloc(
            AllocParams::new(layout.size() * 2).as_type(ramtype::KERNEL_DATA)
        ).ok_or(())?;
        unsafe { talc.claim(ptr.into_slice::<u8>().into())?; }
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
        AllocParams::new(heap_size).as_type(ramtype::KERNEL_DATA)
    ).unwrap();
    unsafe { ALLOCATOR.lock().claim(heap_ptr.into_slice::<u8>().into()).unwrap(); }
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
