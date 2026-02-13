pub mod glacier;
pub mod mutex;
pub mod physalloc;
pub mod reloc;

use crate::{
    arch::rvm::flags,
    kargs::{AP_LIST, KINFO, RAMType},
    printk, printlnk,
    ram::{
        glacier::{GLACIER, hihalf, page_size},
        physalloc::{AllocParams, OwnedPtr, PHYS_ALLOC}
    }
};

use core::{
    alloc::Layout,
    ops::{Deref, DerefMut}
};
use spin::Mutex;
use talc::{OomHandler, Span, Talc, Talck};

// RAM Layout

// VA_BITS =    Virtual address bits(defined by architecture) - typically 48, 52, or 57
// HIHALF =     !0 << (VA_BITS - 1)
// HI_END =     0x1_0000_0000_0000_0000 = 1 << usize::BITS
// PAGE_SIZE =  Page size(defined by architecture) - typically 4 kiB, 16 kiB, or 64 kiB
// KINFO.size = Size of the kernel image
// KHEAP.size = Size of the kernel heap

// PER_CPU_DATA = (PAGE_SIZE << 4).max(0x40000)
// STACK_SIZE =   PAGE_SIZE.max(0x4000)

// Top of virtual RAM
// +------------------+ - HI_END
// |      gleam       |     Global Emergency Access Map
// +------------------+ - HI_END - (PER_CPU_DATA * 1)
// |  per-cpu data 0  |     Per-CPU data region for CPU 0
// +------------------+ - HI_END - (PER_CPU_DATA * 2)
// |  per-cpu data 1  |     Per-CPU data region for CPU 1
// +------------------+ - HI_END - (PER_CPU_DATA * 3) ...
// |       ...        |     etc.
// +------------------+ - HIHALF + KINFO.size + KHEAP.size
// |       heap       |     Dynamic kernel heap
// +------------------+ - HIHALF + KINFO.size
// |      kernel      |     UNIX V11 kernel image
// +------------------+ - HIHALF
// Bottom of Hi-Half

// Top of Lo-Half
// +----------+    +----------+ - !HIHALF + 1
// |  id-map  | or |   user   |
// +----------+    +----------+ - 0x0
// Bottom of virtual RAM

// per-cpu data
// +------------------+ - PER_CPU_DATA
// |      stack       |     Per-CPU kernel stack
// +------------------+ - PER_CPU_DATA - STACK_SIZE
// |    guard page    |     Guard page (not mapped)
// +------------------+ - PER_CPU_DATA - STACK_SIZE - PAGE_SIZE
// |     cpu info     |     Per-CPU mappings and structures
// +------------------+ - 0x0

#[inline(always)]
pub fn stack_size() -> usize {
    return page_size().max(0x4000);
}

#[inline(always)]
pub fn per_cpu_data() -> usize {
    return (page_size() << 4).max(0x40000);
}

pub const PAGE_4KIB: usize = 0x1000;

// Base addr of Global Emergency Access Map
#[inline(always)]
pub fn gleam_base() -> usize {
    return 0usize.wrapping_sub(per_cpu_data());
}

// For DMA or other physical page-aligned buffers
pub struct PhysPageBuf(OwnedPtr);

impl PhysPageBuf {
    pub fn new(size: usize) -> Option<Self> {
        let ptr = PHYS_ALLOC.alloc(
            AllocParams::new(size)
                .align(page_size())
                .as_type(RAMType::KernelData)
        )?;
        return Some(Self(ptr));
    }
}

impl Drop for PhysPageBuf {
    fn drop(&mut self) {
        PHYS_ALLOC.free(unsafe { self.0.clone() });
    }
}

impl Deref for PhysPageBuf {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        return self.0.into_slice();
    }
}

impl DerefMut for PhysPageBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        return self.0.into_slice_mut();
    }
}

pub struct VirtPageBuf {
    ptr: *mut u8,
    layout: Layout
}

unsafe impl Send for VirtPageBuf {}
unsafe impl Sync for VirtPageBuf {}

impl VirtPageBuf {
    pub fn new(size: usize) -> Option<Self> {
        let layout = Layout::from_size_align(size, page_size()).ok()?;
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        return Some(Self { ptr, layout });
    }
}

impl Drop for VirtPageBuf {
    fn drop(&mut self) {
        unsafe { alloc::alloc::dealloc(self.ptr, self.layout); }
    }
}

impl Deref for VirtPageBuf {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }
}

impl DerefMut for VirtPageBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
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
        let size = align_up(layout.size() * 2, page_size());
        let mut rem = size;

        while rem > 0 {
            let mut try_sz = rem;
            let ptr = loop {
                match PHYS_ALLOC.alloc(
                    AllocParams::new(try_sz)
                        .as_type(RAMType::KernelData)
                        .align(page_size())
                ) {
                    Some(p) => break p,
                    None => {
                        if try_sz > page_size() {
                            try_sz = (try_sz / (page_size() << 1)) * page_size();
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

pub fn align_down(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return (val / align) * align;
}

pub fn align_up(val: usize, align: usize) -> usize {
    if align == 0 { return val; }
    return val.div_ceil(align) * align;
}

pub fn size_align(val: usize) -> usize {
    if val < page_size() {
        return val.next_power_of_two();
    }
    return align_up(val, page_size());
}

pub fn dump_bytes(buf: &[u8]) {
    const LINE: usize = 16;
    let mut offset = 0;
    for line in buf.chunks(LINE) {
        printk!("{:08x}  ", offset);
        for (i, byte) in line.iter().enumerate() {
            if i == LINE / 2 { printk!(" "); }
            printk!("{:02x} ", byte);
        }
        for i in line.len()..LINE {
            if i == LINE / 2 { printk!(" "); }
            printk!("   ");
        }
        printk!("   |");
        for byte in line { printk!("{}",
            if (0x20..0x7f).contains(byte) { *byte as char } else { '.' }
        ); }
        printlnk!("|");
        offset += line.len();
    }
    printlnk!("{:08x}", offset);
}

pub fn init_heap() {
    let heap_base = align_up(
        KINFO.read().size + hihalf(),
        page_size()
    );
    KHEAP.lock().oom_handler.set_base(heap_base);
}

pub fn stack_top() -> usize {
    return gleam_base() - (AP_LIST.virtid_self() * per_cpu_data());
}
