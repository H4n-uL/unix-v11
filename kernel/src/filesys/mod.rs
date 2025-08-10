pub mod devfs;
pub mod vfs;

use crate::{
    device::block::BLOCK_DEVICES,
    filesys::{devfs::{ConsoleDevice, DevFS, Device, NullDevice, ZeroDevice}, vfs::{NodeType, VFS}}
};
use core::fmt;
use alloc::{format, string::String, sync::Arc};

pub type Result<T> = core::result::Result<T, FsError>;

#[derive(Debug, Clone)]
pub enum FsError {
    NotFound,
    PermissionDenied,
    NotDirectory,
    NotFile,
    AlreadyExists,
    DirectoryNotEmpty,
    InvalidPath,
    IoError(String),
    NotSupported,
    DeviceError(String)
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotFound => write!(f, "File not found"),
            FsError::PermissionDenied => write!(f, "Permission denied"),
            FsError::NotDirectory => write!(f, "Not a directory"),
            FsError::NotFile => write!(f, "Not a file"),
            FsError::AlreadyExists => write!(f, "Already exists"),
            FsError::DirectoryNotEmpty => write!(f, "Directory not empty"),
            FsError::InvalidPath => write!(f, "Invalid path"),
            FsError::IoError(s) => write!(f, "I/O error: {}", s),
            FsError::NotSupported => write!(f, "Operation not supported"),
            FsError::DeviceError(s) => write!(f, "Device error: {}", s)
        }
    }
}

struct BlockDeviceWrapper {
    device_index: usize
}

impl BlockDeviceWrapper {
    fn new(device_index: usize) -> Self {
        return Self { device_index };
    }
}

impl Device for BlockDeviceWrapper {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        let devices = BLOCK_DEVICES.lock();
        let device = devices.get(self.device_index)
            .ok_or(FsError::DeviceError("Device not found".into()))?;

        let block_size = device.block_size();
        let start_block = offset / block_size;
        let block_offset = offset % block_size;

        let mut temp_buf = alloc::vec![0u8; block_size];

        device.read(start_block as u64, &mut temp_buf)
            .map_err(|e| FsError::DeviceError(e))?;

        let bytes_to_copy = (block_size - block_offset).min(buf.len());
        buf[..bytes_to_copy].copy_from_slice(&temp_buf[block_offset..block_offset + bytes_to_copy]);

        return Ok(bytes_to_copy);
    }

    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        let devices = BLOCK_DEVICES.lock();
        let device = devices.get(self.device_index)
            .ok_or(FsError::DeviceError("Device not found".into()))?;

        let block_size = device.block_size();
        let start_block = offset / block_size;
        let block_offset = offset % block_size;

        let mut temp_buf = alloc::vec![0u8; block_size];

        if block_offset != 0 || buf.len() < block_size {
            device.read(start_block as u64, &mut temp_buf)
                .map_err(|e| FsError::DeviceError(e))?;
        }

        let bytes_to_copy = (block_size - block_offset).min(buf.len());
        temp_buf[block_offset..block_offset + bytes_to_copy].copy_from_slice(&buf[..bytes_to_copy]);

        device.write(start_block as u64, &temp_buf)
            .map_err(|e| FsError::DeviceError(e))?;

        return Ok(bytes_to_copy);
    }

    fn ioctl(&self, cmd: u32, _arg: usize) -> Result<usize> {
        match cmd {
            0x1260 => {
                let devices = BLOCK_DEVICES.lock();
                let device = devices.get(self.device_index)
                    .ok_or(FsError::DeviceError("Device not found".into()))?;
                return Ok(device.block_count());
            }
            0x1268 => {
                let devices = BLOCK_DEVICES.lock();
                let device = devices.get(self.device_index)
                    .ok_or(FsError::DeviceError("Device not found".into()))?;
                return Ok(device.block_size());
            }
            _ => return Err(FsError::NotSupported)
        }
    }
}

pub fn init_filesys() {
    VFS.lock().init();

    let devfs = Arc::new(DevFS::new());
    devfs.add_device("null", Arc::new(NullDevice), NodeType::CharDevice)
        .expect("Failed to add null device");
    devfs.add_device("zero", Arc::new(ZeroDevice), NodeType::CharDevice)
        .expect("Failed to add zero device");
    devfs.add_device("console", Arc::new(ConsoleDevice), NodeType::CharDevice)
        .expect("Failed to add console device");

    for (index, _device) in BLOCK_DEVICES.lock().iter().enumerate() {
        let name = format!("block{}", index);
        devfs.add_device(
            &name,
            Arc::new(BlockDeviceWrapper::new(index)),
            NodeType::BlockDevice
        ).expect("Failed to add block device");
    }

    VFS.lock().create("/dev", NodeType::Directory).expect("Failed to create /dev directory");
    VFS.lock().mount("/dev", devfs).expect("Failed to mount devfs");
}