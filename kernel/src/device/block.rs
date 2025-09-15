use alloc::{string::String, sync::Arc, vec::Vec};
use spin::Mutex;

pub trait BlockDevice: Send + Sync {
    fn block_size(&self) -> u64;
    fn block_count(&self) -> u64;
    fn read_block(&self, buf: &mut [u8], lba: u64) -> Result<(), String>;
    fn write_block(&self, buf: &[u8], lba: u64) -> Result<(), String>;
}

pub static BLOCK_DEVICES: Mutex<Vec<Arc<dyn BlockDevice>>> = Mutex::new(Vec::new());