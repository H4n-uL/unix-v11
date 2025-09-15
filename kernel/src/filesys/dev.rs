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

    pub fn total_size(&self) -> u64 {
        self.block_size() * self.block_count()
    }

    pub fn read_in(&self, buf_len: usize, offset: u64) -> Result<PageAligned, String> {
        let bs = self.block_size();
        let (start, end) = (offset / bs, (offset + buf_len as u64).div_ceil(bs));
        let mut pabuf = PageAligned::new(((end - start) * bs) as usize);
        self.read_block(&mut pabuf, start)?;
        return Ok(pabuf);
    }
}

impl BlockDevice for DevFile {
    fn block_size(&self) -> u64 {
        self.dev.block_size()
    }

    fn block_count(&self) -> u64 {
        self.dev.block_count()
    }

    fn read_block(&self, buf: &mut [u8], lba: u64) -> Result<(), String> {
        self.dev.read_block(buf, lba)
    }

    fn write_block(&self, buf: &[u8], lba: u64) -> Result<(), String> {
        self.dev.write_block(buf, lba)
    }
}

impl VirtFNode for DevFile {
    fn meta(&self) -> FMeta {
        return self.meta.clone();
    }

    fn read(&self, buf: &mut [u8], offset: u64) -> Result<(), String> {
        let temp_buf = self.read_in(buf.len(), offset)?;
        buf.copy_from_slice(&temp_buf[(offset % self.block_size()) as usize..][..buf.len()]);
        return Ok(());
    }

    fn write(&self, buf: &[u8], offset: u64) -> Result<(), String> {
        let mut temp_buf = self.read_in(buf.len(), offset)?;
        temp_buf[(offset % self.block_size()) as usize..][..buf.len()].copy_from_slice(buf);
        return self.dev.write_block(&temp_buf, offset / self.block_size());
    }

    fn truncate(&self, _: u64) -> Result<(), String> {
        return Err("This is not a file.".into());
    }

    fn list(&self) -> Option<Vec<String>> {
        return None;
    }

    fn walk(&self, _: &str) -> Option<Arc<dyn VirtFNode>> {
        return None;
    }

    fn create(&self, _: &str, _: Arc<dyn VirtFNode>) -> Result<(), String> {
        return Err("This is not a directory.".into());
    }

    fn remove(&self, _: &str) -> Result<(), String> {
        return Err("This is not a directory.".into());
    }
}