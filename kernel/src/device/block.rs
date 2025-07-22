use alloc::{boxed::Box, string::String, vec::Vec};
use spin::Mutex;

pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> usize;
    fn read(&self, lba: u64, buffer: &mut [u8]) -> Result<(), String>;
    fn write(&self, lba: u64, buffer: &[u8]) -> Result<(), String>;
}

pub static BLOCK_DEVICES: Mutex<Vec<Box<dyn BlockDevice>>> = Mutex::new(Vec::new());