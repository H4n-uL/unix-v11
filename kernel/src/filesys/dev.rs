use crate::{device::block::BlockDevice, filesys::vfn::{FMeta, FType, VirtFNode}, ram::PageAligned};
use alloc::{string::String, sync::Arc, vec::Vec};

pub struct DevFile {
    dev: Arc<dyn BlockDevice>,
    meta: FMeta
}

impl DevFile {
    pub fn new(dev: Arc<dyn BlockDevice>) -> Self {
        let mut s = Self { dev, meta: FMeta::default(FType::Device) };
        s.meta.size = s.total_size();
        return s;
    }
}

impl DevFile {
    pub fn block_size(&self) -> u64 {
        self.dev.block_size()
    }

    pub fn block_count(&self) -> u64 {
        self.dev.block_count()
    }

    pub fn total_size(&self) -> u64 {
        self.block_size() * self.block_count()
    }
}

impl VirtFNode for DevFile {
    fn meta(&self) -> FMeta {
        return self.meta.clone();
    }

    fn read(&self, buf: &mut [u8], offset: u64) -> bool {
        let bs = self.block_size() as u64;
        let (start, end) = (offset / bs, (offset + buf.len() as u64).div_ceil(bs));
        let mut tempbuf = PageAligned::new(((end - start) * bs) as usize);
        if self.dev.read(&mut tempbuf, start).is_err() {
            return false;
        }
        buf.copy_from_slice(&tempbuf[(offset % bs) as usize..][..buf.len()]);
        return true;
    }

    fn write(&self, buf: &[u8], offset: u64) -> bool {
        let bs = self.block_size() as u64;
        let (start, end) = (offset / bs, (offset + buf.len() as u64).div_ceil(bs));
        let mut tempbuf = PageAligned::new(((end - start) * bs) as usize);
        if self.dev.read(&mut tempbuf, start).is_err() {
            return false;
        }
        tempbuf[(offset % bs) as usize..][..buf.len()].copy_from_slice(buf);
        return self.dev.write(&tempbuf, start).is_ok();
    }

    fn truncate(&self, _: u64) -> bool {
        return false;
    }

    fn list(&self) -> Option<Vec<String>> {
        return None;
    }

    fn walk(&self, _: &str) -> Option<Arc<dyn VirtFNode>> {
        return None;
    }

    fn create(&self, _: &str, _: Arc<dyn VirtFNode>) -> bool {
        return false;
    }

    fn remove(&self, _: &str) -> bool {
        return false;
    }
}