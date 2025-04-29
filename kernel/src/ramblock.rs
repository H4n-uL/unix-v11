use crate::{arch, ember::{ramtype, RAMDescriptor}, ram::PAGE_4KIB};
use spin::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlock {
    pub addr: usize,
    pub size: usize,
    pub used: bool
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlockManager {
    blocks: *mut Option<RAMBlock>,
    is_init: bool,
    count: usize,
    max: usize
}

pub const BASE_RAMBLOCK_SIZE: usize = 128;
pub static RAM_BLOCKS: Mutex<[Option<RAMBlock>; BASE_RAMBLOCK_SIZE]> = Mutex::new([None; BASE_RAMBLOCK_SIZE]);
pub static RAM_BLOCK_MANAGER: Mutex<RAMBlockManager> = Mutex::new(RAMBlockManager {
    blocks: &raw const RAM_BLOCKS as *mut _,
    is_init: false, count: 0,
    max: BASE_RAMBLOCK_SIZE,
});
unsafe impl Send for RAMBlockManager {}
unsafe impl Sync for RAMBlockManager {}

impl RAMBlockManager {
    pub fn init(&mut self, ram_layout: &[RAMDescriptor]) {
        if self.is_init { return; } self.count = 0;
        for desc in ram_layout {
            if desc.ty == ramtype::CONVENTIONAL {
                self.add(desc.phys_start as usize, desc.page_count as usize * PAGE_4KIB);
            }
        }
    }

    pub fn blocks(&self) -> &[Option<RAMBlock>] {
        unsafe { core::slice::from_raw_parts(self.blocks, self.count) }
    }

    fn blocks_mut(&mut self) -> &mut [Option<RAMBlock>] {
        unsafe { core::slice::from_raw_parts_mut(self.blocks, self.count) }
    }

    pub fn sort(&mut self) {
        use crate::sort::HeaplessSort;
        self.blocks_mut().sort_noheap_by(|a, b| match (a, b) {
            (Some(a), Some(b)) => a.addr.cmp(&b.addr),
            (Some(_), None) => core::cmp::Ordering::Less,
            (None, Some(_)) => core::cmp::Ordering::Greater,
            (None, None) => core::cmp::Ordering::Equal,
        });
    }

    pub fn find_free_ram(&mut self, size: usize) -> Option<usize> {
        return self.blocks().iter().flatten()
            .find(|block| !block.used && block.size >= size)
            .map(|block| block.addr);
    }

    pub fn add(&mut self, addr: usize, size: usize) {
        if self.count >= self.max { self.expand(self.max * 2); }
        let idx = self.count; self.count += 1;
        let blocks = self.blocks_mut();
        blocks[idx] = Some(RAMBlock { addr, size, used: false });

        for i in (1..=idx).rev() {
            if let (Some(current), Some(prev)) = (blocks[i], blocks[i - 1]) {
                if current.addr < prev.addr { blocks.swap(i, i - 1); }
                else { break; }
            }
            else { break; }
        }
    }

    pub fn alloc_at(&mut self, addr: usize, size: usize) {
        for block_opt in self.blocks_mut() {
            if let Some(block) = block_opt {
                let block_start = block.addr;
                let block_end = block.addr + block.size;
                let req_start = addr;
                let req_end = addr + size;

                if req_start >= block_start && req_end <= block_end {
                    let before_size = req_start - block_start;
                    let after_size = block_end - req_end;

                    *block_opt = Some(RAMBlock { addr: req_start, size, used: true });
                    if before_size > 0 { self.add(block_start, before_size); }
                    if after_size > 0 { self.add(req_end, after_size); }
                    return;
                }
            }
        }
        arch::serial_puts("Tried to allocate unknown block\n");
    }

    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        let ptr = self.find_free_ram(size);
        if let Some(addr) = ptr { self.alloc_at(addr, size); }
        return ptr;
    }

    pub fn free(&mut self, addr: usize) {
        let found = self.blocks_mut().iter_mut().flatten()
            .find(|block| block.addr <= addr && block.addr + block.size > addr);

        if let Some(block) = found { block.used = false; }
        else { arch::serial_puts("Tried to free unknown block\n"); }
    }

    pub fn expand(&mut self, new_max: usize) {
        if new_max <= self.max { return; }

        let manager_size = new_max * core::mem::size_of::<Option<RAMBlock>>();
        let old_blocks_ptr = self.blocks;
        let new_blocks_ptr = self.find_free_ram(manager_size).unwrap() as *mut Option<RAMBlock>;
        unsafe {
            core::ptr::copy(self.blocks().as_ptr(), new_blocks_ptr, self.count);
            self.blocks = new_blocks_ptr;
            self.max = new_max;
        }
        self.alloc_at(new_blocks_ptr as usize, manager_size);
        self.free(old_blocks_ptr as usize);
    }
}