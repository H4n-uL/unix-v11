use crate::{
    device::block::{BlockDevice, DevId},
    filesys::vfn::{vfid, FMeta, FType, VirtFNode}
};

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
        let bs = self.block_size();
        let (start, end) = (offset / bs, (offset + buf.len() as u64).div_ceil(bs));
        let mut vec = alloc::vec![0; ((end - start) * bs) as usize];

        self.read_block(&mut vec, start)?;

        buf.copy_from_slice(&vec[(offset % bs) as usize..][..buf.len()]);
        return Ok(());
    }

    fn write(&self, buf: &[u8], offset: u64) -> Result<(), String> {
        let bs = self.block_size();
        let (start, end) = (offset / bs, (offset + buf.len() as u64).div_ceil(bs));
        let mut vec = alloc::vec![0; ((end - start) * bs) as usize];
        let len = vec.len();

        self.read_block(&mut vec[..bs as usize], start)?;
        self.read_block(&mut vec[(len - bs as usize)..], end - 1)?;

        vec[(offset % bs) as usize..][..buf.len()].copy_from_slice(buf);
        return self.write_block(&vec, offset / bs);
    }

    fn truncate(&self, _: u64) -> Result<(), String> {
        return Err("This is not a file".into());
    }

    fn as_blkdev(&self) -> Option<Arc<dyn BlockDevice>> {
        Some(Arc::new(self.clone()))
    }
}

#[derive(Clone)]
pub struct PartDev {
    dev: Arc<dyn BlockDevice>,
    meta: FMeta,
    devid: u64,
    start_lba: u64,
    block_count: u64,
}

impl PartDev {
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
}

impl BlockDevice for PartDev {
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

impl VirtFNode for PartDev {
    fn meta(&self) -> FMeta {
        return self.meta.clone();
    }

    fn read(&self, buf: &mut [u8], offset: u64) -> Result<(), String> {
        let bs = self.block_size();
        let (start, end) = (offset / bs, (offset + buf.len() as u64).div_ceil(bs));
        let mut vec = alloc::vec![0; ((end - start) * bs) as usize];

        self.read_block(&mut vec, start)?;

        buf.copy_from_slice(&vec[(offset % bs) as usize..][..buf.len()]);
        return Ok(());
    }

    fn write(&self, buf: &[u8], offset: u64) -> Result<(), String> {
        let bs = self.block_size();
        let (start, end) = (offset / bs, (offset + buf.len() as u64).div_ceil(bs));
        let mut vec = alloc::vec![0; ((end - start) * bs) as usize];
        let len = vec.len();

        self.read_block(&mut vec[..bs as usize], start)?;
        self.read_block(&mut vec[(len - bs as usize)..], end - 1)?;

        vec[(offset % bs) as usize..][..buf.len()].copy_from_slice(buf);
        return self.write_block(&vec, offset / bs);
    }

    fn truncate(&self, _: u64) -> Result<(), String> {
        return Err("This is not a file".into());
    }

    fn as_blkdev(&self) -> Option<Arc<dyn BlockDevice>> {
        Some(Arc::new(self.clone()))
    }
}
