use crate::{device::block::{BlockDevice, DevId}, filesys::vfn::{vfid, FMeta, FType, VirtFNode}, ram::PageAligned};
use alloc::{string::String, sync::Arc};

#[derive(Clone)]
pub struct DevFile {
    dev: Arc<dyn BlockDevice>,
    meta: FMeta
}

impl DevFile {
    pub fn new(dev: Arc<dyn BlockDevice>) -> Self {
        let meta = FMeta::default(vfid(), 1, FType::Device);
        let mut s = Self { dev, meta };
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

    fn devid(&self) -> u64 {
        self.dev.devid()
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
        return Err("This is not a file".into());
    }

    fn as_blkdev(&self) -> Option<Arc<dyn BlockDevice>> {
        Some(Arc::new(self.clone()))
    }
}

#[derive(Clone)]
pub struct PartitionDev {
    dev: Arc<dyn BlockDevice>,
    meta: FMeta,
    devid: u64,
    start_lba: u64,
    block_count: u64,
}

impl PartitionDev {
    pub fn new(dev: Arc<dyn BlockDevice>, part_no: u32, start_lba: u64, block_count: u64) -> Self {
        let devid = DevId::new(dev.devid()).part(part_no).build();
        let meta = FMeta::default(vfid(), 1, FType::Partition);
        let mut s = Self { dev, meta, devid, start_lba, block_count };
        s.meta.size = s.total_size();
        return s;
    }

    pub fn total_size(&self) -> u64 {
        self.block_size() * self.block_count()
    }

    pub fn read_in(&self, buf: &[u8], offset: u64) -> Result<PageAligned, String> {
        let bs = self.block_size();
        let (start, end) = (offset / bs, (offset + buf.len() as u64).div_ceil(bs));
        let mut tempbuf = PageAligned::new(((end - start) * bs) as usize);
        self.dev.read_block(&mut tempbuf, start)?;
        return Ok(tempbuf);
    }
}

impl BlockDevice for PartitionDev {
    fn block_size(&self) -> u64 {
        self.dev.block_size()
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn read_block(&self, buf: &mut [u8], lba: u64) -> Result<(), String> {
        self.dev.read_block(buf, lba + self.start_lba)
    }

    fn write_block(&self, buf: &[u8], lba: u64) -> Result<(), String> {
        self.dev.write_block(buf, lba + self.start_lba)
    }

    fn devid(&self) -> u64 {
        self.devid
    }
}

impl VirtFNode for PartitionDev {
    fn meta(&self) -> FMeta {
        return self.meta.clone();
    }

    fn read(&self, buf: &mut [u8], offset: u64) -> Result<(), String> {
        let offset = offset + self.start_lba * self.block_size();
        let temp_buf = self.read_in(buf, offset)?;
        buf.copy_from_slice(&temp_buf[(offset % self.block_size()) as usize..][..buf.len()]);
        return Ok(());
    }

    fn write(&self, buf: &[u8], offset: u64) -> Result<(), String> {
        let offset = offset + self.start_lba * self.block_size();
        let mut temp_buf = self.read_in(buf, offset)?;
        temp_buf[(offset % self.block_size()) as usize..][..buf.len()].copy_from_slice(buf);
        return self.dev.write_block(&temp_buf, offset / self.block_size());
    }

    fn truncate(&self, _: u64) -> Result<(), String> {
        return Err("This is not a file".into());
    }

    fn as_blkdev(&self) -> Option<Arc<dyn BlockDevice>> {
        Some(Arc::new(self.clone()))
    }
}
