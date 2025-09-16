use core::sync::atomic::{AtomicU64, Ordering as SyncOrd};
use alloc::{string::String, sync::Arc, vec::Vec};

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FType {
    Regular = 0,
    Directory = 1,
    Device = 2,
    Partition = 3
}

#[repr(C)]
#[derive(Clone)]
pub struct FMeta {
    pub fid: u64,
    pub partid: u64,
    pub size: u64,
    pub ftype: FType,
    pub perm: u16,
    pub uid: u16,
    pub gid: u16
}


static FID: AtomicU64 = AtomicU64::new(0);

impl FMeta {
    pub fn vfs_only(ftype: FType) -> Self {
        return Self::default(FID.fetch_add(1, SyncOrd::SeqCst), 0, ftype);
    }

    pub fn default(fid: u64, partid: u64, ftype: FType) -> Self {
        let perm = match ftype {
            FType::Regular => 0x644,
            FType::Directory => 0x755,
            FType::Device => 0x640,
            FType::Partition => 0x640
        };
        return Self {
            fid, partid,
            size: 0, ftype, perm,
            uid: 0, gid: 0
        };
    }
}

// INTENSIONALLY FORCING INTERIOR MUTABILITY
pub trait VirtFNode: Send + Sync {
    fn meta(&self) -> FMeta;
    fn read(&self, buf: &mut [u8], offset: u64) -> Result<(), String>;
    fn write(&self, buf: &[u8], offset: u64) -> Result<(), String>;
    fn truncate(&self, size: u64) -> Result<(), String>;
    fn list(&self) -> Option<Vec<String>>;
    fn walk(&self, name: &str) -> Option<Arc<dyn VirtFNode>>;
    fn create(&self, name: &str, node: Arc<dyn VirtFNode>) -> Result<(), String>;
    fn remove(&self, name: &str) -> Result<(), String>;
}