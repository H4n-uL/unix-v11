use crate::{arch, ember::{ramtype, RAMDescriptor}, ram::PAGE_4KIB};
use spin::Mutex;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlock {
    pub base: usize,
    pub size: usize,
    pub used: bool
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMBlockManager {
    pub blocks: *mut Option<RAMBlock>,
    pub count: usize,
    pub max: usize,
}

pub const BASE_RAMBLOCK_SIZE: usize = 128;
pub static RAM_BLOCKS: Mutex<[Option<RAMBlock>; BASE_RAMBLOCK_SIZE]> = Mutex::new([None; BASE_RAMBLOCK_SIZE]);
pub static RAM_BLOCK_MANAGER: Mutex<RAMBlockManager> = Mutex::new(RAMBlockManager {
    blocks: &raw const RAM_BLOCKS as *const _ as *mut _,
    count: 0,
    max: BASE_RAMBLOCK_SIZE,
});
unsafe impl Send for RAMBlockManager {}
unsafe impl Sync for RAMBlockManager {}

impl RAMBlockManager {
    pub fn init(&mut self, ram_layout: &[RAMDescriptor]) {
        for desc in ram_layout {
            if desc.ty == ramtype::CONVENTIONAL {
                self.add(desc.phys_start as usize, desc.page_count as usize * PAGE_4KIB);
            }
        }
    }

    pub fn add(&mut self, base: usize, size: usize) {
        if self.count >= self.max { self.expand(self.max * 2); }
        unsafe {
            let blocks = core::slice::from_raw_parts_mut(self.blocks, self.max);
            blocks[self.count] = Some(RAMBlock { base, size, used: false });
        }
        self.count += 1;
    }

    pub fn reserve(&mut self, base: usize, size: usize) {
        unsafe {
            let blocks = core::slice::from_raw_parts_mut(self.blocks, self.max);
            for i in 0..self.count {
                if let Some(block) = blocks[i] {
                    let block_start = block.base;
                    let block_end = block.base + block.size;
                    let req_start = base;
                    let req_end = base + size;

                    if req_start >= block_start && req_end <= block_end {
                        let before_size = req_start - block_start;
                        let after_size = block_end - req_end;

                        blocks[i] = Some(RAMBlock {
                            base: req_start,
                            size,
                            used: true,
                        });

                        if before_size > 0 { self.add(block_start, before_size); }
                        if after_size > 0 { self.add(req_end, after_size); }
                        return;
                    }
                }
            }
        }
        arch::serial_puts("Tried to reserve unknown block\n");
    }

    pub fn free(&mut self, base: usize) {
        unsafe {
            let blocks = core::slice::from_raw_parts_mut(self.blocks, self.max);
            for block in blocks.iter_mut().flatten() {
                if block.base == base {
                    block.used = false;
                    return;
                }
            }
        }
        arch::serial_puts("Tried to free unknown block\n");
    }

    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        unsafe {
            let blocks = core::slice::from_raw_parts_mut(self.blocks, self.max);
            for block in blocks.iter_mut().flatten() {
                if !block.used && block.size >= size {
                    block.used = true;
                    return Some(block.base);
                }
            }
        }
        return None;
    }

    pub fn expand(&mut self, new_max: usize) {
        if new_max <= self.max { return; }

        let needed_bytes = new_max * core::mem::size_of::<Option<RAMBlock>>();
        let new_blocks_ptr = self.find_free_ram(needed_bytes) as *mut Option<RAMBlock>;

        unsafe {
            let old_blocks = core::slice::from_raw_parts(self.blocks, self.max);
            let new_blocks = core::slice::from_raw_parts_mut(new_blocks_ptr, new_max);

            for i in 0..self.count {
                new_blocks[i] = old_blocks[i];
            }

            self.blocks = new_blocks_ptr;
            self.max = new_max;
        }
    }

    pub fn find_free_ram(&mut self, needed_bytes: usize) -> usize {
        unsafe {
            let blocks = core::slice::from_raw_parts_mut(self.blocks, self.max);
            for block in blocks.iter_mut().flatten() {
                if !block.used && block.size >= needed_bytes {
                    let base = block.base;

                    block.base += needed_bytes;
                    block.size -= needed_bytes;

                    return base;
                }
            }
        }
        arch::serial_puts("No free RAM block big enough for");
        arch::serial_puthex(needed_bytes);
        arch::serial_puts(" bytes\n"); panic!();
    }

    pub fn blocks(&self) -> &[Option<RAMBlock>] {
        unsafe { core::slice::from_raw_parts(self.blocks, self.count) }
    }
}