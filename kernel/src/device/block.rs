use alloc::{string::String, sync::Arc, vec::Vec};
use spin::Mutex;

pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> u64;
    fn block_count(&self) -> u64;
    fn read_block(&self, buf: &mut [u8], lba: u64) -> Result<(), String>;
    fn write_block(&self, buf: &[u8], lba: u64) -> Result<(), String>;
    fn devid(&self) -> u64; // [Type:8][Location:32][Partition:24]
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDevType {
    PCIe = 0x01,    // Location: [BDF:16][Subdevice:16]
    USB = 0x02,     // Location: ?
    RamDisk = 0x03, // Location: ?
    Legacy = 0x04   // Location: ?
}

pub struct DevId(u64);

impl DevId {
    pub fn new(init: u64) -> Self {
        Self(init)
    }

    pub fn ty(mut self, dev_type: BlockDevType) -> Self {
        self.0 &= !(0xff << 56);
        self.0 |= (dev_type as u64) << 56;
        self
    }

    pub fn loc(mut self, loc: u32) -> Self {
        self.0 &= !(0xffffffff << 24);
        self.0 |= (loc as u64) << 24;
        self
    }

    pub fn part(mut self, part: u32) -> Self {
        self.0 &= !(0xffffff);
        self.0 |= (part as u64 + 1) & 0xffffff;
        self
    }

    pub fn build(&self) -> u64 {
        self.0
    }
}

pub static BLOCK_DEVICES: Mutex<Vec<Arc<dyn BlockDevice>>> = Mutex::new(Vec::new());
