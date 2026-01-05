pub mod glacier;
pub mod physalloc;
pub mod reloc;

use crate::{
    arch::rvm::flags,
    kargs::{RAMType, ap_vid},
    ram::{
        glacier::GLACIER,
        physalloc::{AllocParams, PHYS_ALLOC}
    }
};

use core::{alloc::Layout, ops::{Deref, DerefMut}};
use spin::Mutex;
use talc::{OomHandler, Span, Talc, Talck};

pub const PAGE_4KIB: usize = 0x1000;
pub const STACK_SIZE: usize = 0x4000;

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

pub struct KheapHandler {
    base: usize,
    heap: Span
}

impl KheapHandler {
    const fn new() -> Self {
        return Self {
            base: 0,
            heap: Span::empty()
        };
    }

    const fn base(&self) -> usize {
        return self.base;
    }

    const fn heap(&self) -> Span {
        return self.heap;
    }

    const fn set_base(&mut self, base: usize) {
        self.base = base;
    }

    const fn set_heap(&mut self, heap: Span) {
        self.heap = heap;
    }

    fn size(&self) -> usize {
        return self.heap.size();
    }
}

impl OomHandler for KheapHandler {
    fn handle_oom(talc: &mut Talc<Self>, layout: Layout) -> Result<(), ()> {
        let size = align_up(layout.size() * 2, PAGE_4KIB);
        let mut rem = size;

        while rem > 0 {
            let mut try_sz = rem;
            let ptr = loop {
                match PHYS_ALLOC.alloc(
                    AllocParams::new(try_sz)
                        .as_type(RAMType::KernelData)
                        .align(PAGE_4KIB)
                ) {
                    Some(p) => break p,
                    None => {
                        if try_sz > PAGE_4KIB {
                            try_sz = (try_sz / (2 * PAGE_4KIB)) * PAGE_4KIB;
                        } else {
                            return Err(());
                        }
                    }
                }
            };

            unsafe {
                let khh = &mut talc.oom_handler;

                GLACIER.write().map_range(
                    khh.base() + khh.size(),
                    ptr.addr(),
                    ptr.size(),
                    flags::K_RWO
                );
                GLACIER.write().unmap_range(
                    ptr.addr(),
                    ptr.size()
                );

                if khh.heap.is_empty() {
                    let heap = Span::from(core::slice::from_raw_parts(
                        khh.base() as *const u8, ptr.size()
                    ));
                    khh.set_heap(heap);
                    talc.claim(heap)?;
                } else {
                    let old_heap = khh.heap();
                    let new_heap = old_heap.extend(0, ptr.size());
                    khh.set_heap(new_heap);
                    talc.extend(old_heap, new_heap);
                }
            }

            rem = rem.saturating_sub(ptr.size());
        }

        return Ok(());
    }
}

#[global_allocator]
pub static KHEAP: Talck<Mutex<()>, KheapHandler> = Talc::new(KheapHandler::new()).lock();

pub fn align_up(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return val.div_ceil(align) * align;
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

pub fn stack_top() -> usize {
    return 0usize.wrapping_sub(ap_vid() * (STACK_SIZE << 1)) - STACK_SIZE;
}
