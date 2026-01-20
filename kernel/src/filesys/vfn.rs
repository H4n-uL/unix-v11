use crate::device::block::BlockDevice;

use core::sync::atomic::{AtomicU64, Ordering as SyncOrd};
use alloc::{string::String, sync::Arc, vec::Vec};

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FType {
    Fifo = 1,
    CharDev = 2,
    Directory = 4,
    BlockDev = 6,
    Regular = 8,
    SymLink = 0xa,
    Socket = 0xc
}

#[repr(C)]
#[derive(Clone)]
pub struct FMeta {
    pub fid: u64,
    pub hostdev: u64,
    pub size: u64,
    pub ftype: FType,
    pub perm: u16,
    pub uid: u16,
    pub gid: u16
}

static FID: AtomicU64 = AtomicU64::new(2);

pub fn vfid() -> u64 {
    return FID.fetch_add(1, SyncOrd::SeqCst);
}

impl FMeta {
    pub fn vfs_only(ftype: FType) -> Self {
        return Self::default(vfid(), 0, ftype);
    }

    pub fn default(fid: u64, hostdev: u64, ftype: FType) -> Self {
        let perm = match ftype {
            FType::Regular => 0x644,
            FType::Directory => 0x755,
            FType::BlockDev => 0x640,
            FType::CharDev => 0x640,
            FType::Fifo => 0x644,
            FType::SymLink => 0o777,
            FType::Socket => 0x644
        };
        return Self {
            fid, hostdev,
            size: 0, ftype, perm,
            uid: 0, gid: 0
        };
    }
}

// INTENTIONALLY FORCING INTERIOR MUTABILITY
pub trait VirtFNode: Send + Sync {
    fn meta(&self) -> FMeta;
    fn read(&self, _buf: &mut [u8], _offset: u64) -> Result<(), String> { Err("This file is not IOable".into()) }
    fn write(&self, _buf: &[u8], _offset: u64) -> Result<(), String> { Err("This file is not IOable".into()) }
    fn truncate(&self, _size: u64) -> Result<(), String> { Err("This file is not IOable".into()) }
    fn list(&self) -> Result<Vec<String>, String> { Err("This is not a directory".into()) }
    fn walk(&self, _name: &str) -> Result<Arc<dyn VirtFNode>, String> { Err("This is not a directory".into()) }
    fn create(&self, _name: &str, _ftype: FType) -> Result<(), String> { Err("This is not a directory".into()) }
    fn link(&self, _name: &str, _node: Arc<dyn VirtFNode>) -> Result<(), String> { Err("This is not a directory".into()) }
    fn remove(&self, _name: &str) -> Result<(), String> { Err("This is not a directory".into()) }
    fn as_blkdev(&self) -> Option<Arc<dyn BlockDevice>> { None }
}
