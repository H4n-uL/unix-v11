use crate::{
    arch::rvm::flags,
    ram::{VirtPageBuf, glacier::{GLACIER, page_size}, stack_size}
};

pub struct KernelStack {
    buf: VirtPageBuf,
    pa: usize
}

impl KernelStack {
    pub fn new() -> Option<Self> {
        let mut glacier = GLACIER.write();
        let buf = VirtPageBuf::new(stack_size() + page_size())?;
        let va = buf.as_ptr() as usize;
        let pa = glacier.get_pa(va)?;
        glacier.unmap_page(va);
        return Some(Self { buf, pa });
    }
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        let va = self.buf.as_ptr() as usize;
        let _ = GLACIER.write().map_page(va, self.pa, flags::K_RWO);
    }
}
