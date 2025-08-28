pub mod gpt;
pub mod fat32;

use crate::{
    device::block::BlockDevice,
    filesys::{devfs::Device, FsError, Result}, ram::PageAligned
};
use alloc::sync::Arc;

pub struct PartitionDevice {
    device: Arc<dyn BlockDevice>,
    start_lba: u64
}

impl PartitionDevice {
    pub fn new(device: Arc<dyn BlockDevice>, start_lba: u64) -> Self {
        return Self { device, start_lba };
    }
}

impl Device for PartitionDevice {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        let block_size = self.device.block_size();
        let lba = self.start_lba + (offset / block_size) as u64;
        let lba_offset = offset % block_size;
        let mut temp = PageAligned::new(block_size);
        self.device.read(lba, &mut temp)
            .map_err(|e| FsError::DeviceError(e))?;

        let to_copy = (block_size - lba_offset).min(buf.len());
        buf[..to_copy].copy_from_slice(&temp[lba_offset..lba_offset + to_copy]);

        return Ok(to_copy);
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize> {
        Err(FsError::NotSupported)
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> Result<usize> {
        Err(FsError::NotSupported)
    }
}