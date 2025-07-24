use crate::{arch, filesys::vfs::{self, DirEntry, FileSystem, FsError, Metadata, NodeType, Result, VNode}};
use alloc::{collections::BTreeMap, string::{String, ToString}, sync::Arc, vec::Vec};
use spin::{RwLock};

pub trait Device: Send + Sync {
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn ioctl(&self, cmd: u32, arg: usize) -> Result<usize>;
}

struct DeviceNode {
    name: String,
    device: Arc<dyn Device>,
    metadata: Metadata
}

impl VNode for DeviceNode {
    fn metadata(&self) -> Result<Metadata> {
        return Ok(self.metadata.clone());
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        return self.device.read(offset, buf);
    }

    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        return self.device.write(offset, buf);
    }

    fn readdir(&self) -> Result<Vec<DirEntry>> {
        return Err(FsError::NotDirectory);
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn VNode>> {
        return Err(FsError::NotDirectory);
    }

    fn create(&self, _name: &str, _node_type: NodeType) -> Result<Arc<dyn VNode>> {
        return Err(FsError::NotSupported);
    }

    fn unlink(&self, _name: &str) -> Result<()> {
        return Err(FsError::NotSupported);
    }

    fn truncate(&self, _size: usize) -> Result<()> {
        return Err(FsError::NotSupported);
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> Result<usize> {
        return self.device.ioctl(cmd, arg);
    }
}

struct DevDir {
    devices: RwLock<BTreeMap<String, Arc<DeviceNode>>>
}

impl DevDir {
    fn new() -> Self {
        return Self {
            devices: RwLock::new(BTreeMap::new())
        };
    }

    fn add_device(&self, name: String, device: Arc<dyn Device>, node_type: NodeType) -> Result<()> {
        let mut devices = self.devices.write();

        if devices.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }

        let mut metadata = Metadata::default();
        metadata.node_type = node_type;
        metadata.permissions = 0o666;

        let node = Arc::new(DeviceNode {
            name: name.clone(),
            device,
            metadata
        });

        devices.insert(name, node);
        return Ok(());
    }

    fn remove_device(&self, name: &str) -> Result<()> {
        let mut devices = self.devices.write();
        devices.remove(name).ok_or(FsError::NotFound)?;
        return Ok(());
    }
}

impl VNode for DevDir {
    fn metadata(&self) -> Result<Metadata> {
        let mut metadata = Metadata::default();
        metadata.node_type = NodeType::Directory;
        metadata.permissions = 0o755;
        return Ok(metadata);
    }

    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize> {
        return Err(FsError::NotFile);
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize> {
        return Err(FsError::NotFile);
    }

    fn readdir(&self) -> Result<Vec<DirEntry>> {
        let devices = self.devices.read();
        let entries = devices.iter().map(|(name, node)| {
            DirEntry {
                name: name.clone(),
                node_type: node.metadata.node_type
            }
        }).collect();
        return Ok(entries);
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn VNode>> {
        let devices = self.devices.read();
        let node = devices.get(name).ok_or(FsError::NotFound)?;
        return Ok(node.clone() as Arc<dyn VNode>);
    }

    fn create(&self, _name: &str, _node_type: NodeType) -> Result<Arc<dyn VNode>> {
        return Err(FsError::NotSupported);
    }

    fn unlink(&self, name: &str) -> Result<()> {
        return self.remove_device(name);
    }

    fn truncate(&self, _size: usize) -> Result<()> {
        return Err(FsError::NotDirectory);
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> Result<usize> {
        return Err(FsError::NotSupported);
    }
}

pub struct DevFS {
    root: Arc<DevDir>
}

impl DevFS {
    pub fn new() -> Self {
        return Self {
            root: Arc::new(DevDir::new())
        };
    }

    pub fn add_device(&self, name: &str, device: Arc<dyn Device>, node_type: NodeType) -> Result<()> {
        return self.root.add_device(name.to_string(), device, node_type);
    }

    pub fn remove_device(&self, name: &str) -> Result<()> {
        return self.root.remove_device(name);
    }
}

impl FileSystem for DevFS {
    fn root(&self) -> Arc<dyn VNode> {
        return self.root.clone() as Arc<dyn VNode>;
    }

    fn sync(&self) -> Result<()> {
        return Ok(());
    }
}

pub struct NullDevice;

impl Device for NullDevice {
    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize> {
        return Ok(0);
    }

    fn write(&self, _offset: usize, buf: &[u8]) -> Result<usize> {
        return Ok(buf.len());
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> Result<usize> {
        return Err(FsError::NotSupported);
    }
}

pub struct ZeroDevice;

impl Device for ZeroDevice {
    fn read(&self, _offset: usize, buf: &mut [u8]) -> Result<usize> {
        buf.fill(0);
        return Ok(buf.len());
    }

    fn write(&self, _offset: usize, buf: &[u8]) -> Result<usize> {
        return Ok(buf.len());
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> Result<usize> {
        return Err(FsError::NotSupported);
    }
}

pub struct ConsoleDevice;

impl Device for ConsoleDevice {
    fn read(&self, _offset: usize, _buf: &mut [u8]) -> vfs::Result<usize> {
        return Err(vfs::FsError::NotSupported);
    }

    fn write(&self, _offset: usize, buf: &[u8]) -> vfs::Result<usize> {
        for &byte in buf { arch::serial_putchar(byte); }
        return Ok(buf.len());
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> vfs::Result<usize> {
        return Err(vfs::FsError::NotSupported);
    }
}