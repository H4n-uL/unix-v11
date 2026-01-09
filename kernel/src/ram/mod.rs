pub mod glacier;
pub mod mutex;
pub mod physalloc;
pub mod reloc;

use crate::{
    arch::rvm::flags,
    kargs::{AP_LIST, KINFO, RAMType},
    printk, printlnk,
    ram::{
        glacier::{GLACIER, hihalf},
        physalloc::{AllocParams, OwnedPtr, PHYS_ALLOC}
    }
};

use core::{
    alloc::Layout,
    ops::{Deref, DerefMut}
};
use spin::Mutex;
use talc::{OomHandler, Span, Talc, Talck};

// Top of virtual RAM
// +------------------+ - 0x1_0000_0000_0000_0000
// |      GLEAM       | 64 kiB: global emergency access map
// +------------------+ -   0xffff_ffff_ffff_0000
// |  per-cpu data 0  | 64 kiB: per-cpu data for cpu 0
// +------------------+ -   0xffff_ffff_fffe_0000
// |  per-cpu data 1  | 64 kiB: per-cpu data for cpu 1
// +------------------+ -   0xffff_ffff_fffd_0000
// |       ...        | etc.
// +------------------+ - HIHALF + KINFO.size + KHEAP.size
// |       heap       | variable: kernel heap
// +------------------+ - HIHALF + KINFO.size
// |      Kernel      | variable: UNIX V11 Kernel Image
// +------------------+ - HIHALF
// Bottom of Hi-Half

// Top of Lo-Half
// +------------------+
// |  idmap ||  user  |
// +------------------+
// Bottom of virtual RAM

// per-cpu data
// +------------------+ - 0x1_0000
// |      stack       | 16 kiB: kernel stack
// +------------------+ -   0xc000
// |    guard page    | 4 kiB: unmapped guard page
// +------------------+ -   0xb000
// |     cpu info     | 44 kiB: per-cpu mappings and structures
// +------------------+ -      0x0

pub const PAGE_4KIB: usize = 0x1000;
pub const STACK_SIZE: usize = 0x4000;
pub const PER_CPU_DATA: usize = 0x10000;
const _: () = assert!(PER_CPU_DATA % PAGE_4KIB == 0, "PER_CPU_DATA must be page-aligned");

// Base addr of Global Emergency Access Map
pub const GLEAM_BASE: usize = 0usize.wrapping_sub(PER_CPU_DATA);
const _: () = assert!(GLEAM_BASE % PAGE_4KIB == 0, "GLEAM_BASE must be page-aligned");

// For DMA or other physical page-aligned buffers
pub struct PhysPageBuf(OwnedPtr);

impl PhysPageBuf {
    pub fn new(size: usize) -> Option<Self> {
        let ptr = PHYS_ALLOC.alloc(
            AllocParams::new(size)
                .align(PAGE_4KIB)
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

impl VirtPageBuf {
    pub fn new(size: usize) -> Option<Self> {
        let layout = Layout::from_size_align(size, PAGE_4KIB).ok()?;
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
        PAGE_4KIB
    );
    KHEAP.lock().oom_handler.set_base(heap_base);
}

pub fn stack_top() -> usize {
    return GLEAM_BASE - (AP_LIST.virtid_self() * PER_CPU_DATA);
}
