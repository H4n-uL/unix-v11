use alloc::string::String;

pub trait BlockDevice {
    fn block_size(&self) -> usize;
    fn block_count(&self) -> usize;
    fn read(&self, lba: u64, buffer: &mut [u8]) -> Result<(), String>;
    fn write(&self, lba: u64, buffer: &[u8]) -> Result<(), String>;
}