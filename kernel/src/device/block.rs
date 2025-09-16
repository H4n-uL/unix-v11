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
    PCIe = 0x01,    // Location: [BDF:16][SubDevice:16]
    USB = 0x02,     // Location: ?
    RamDisk = 0x03, // Location: ?
    Legacy = 0x04   // Location: ?
}

pub static BLOCK_DEVICES: Mutex<Vec<Arc<dyn BlockDevice>>> = Mutex::new(Vec::new());