use alloc::string::String;

pub trait BlockDevice {
    fn read(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), String>;
    fn write(&mut self, lba: u64, buffer: &[u8]) -> Result<(), String>;
}