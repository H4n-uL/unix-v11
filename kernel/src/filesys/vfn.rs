use alloc::{string::String, sync::Arc, vec::Vec};

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FType {
    Regular = 0,
    Directory = 1
}

#[repr(C)]
#[derive(Clone)]
pub struct FMeta {
    pub size: u64,
    pub ftype: FType,
    pub perm: u16,
    pub uid: u16,
    pub gid: u16
}

impl FMeta {
    pub fn default(ftype: FType) -> Self {
        let perm = match ftype {
            FType::Regular => 0x644,
            FType::Directory => 0x755
        };
        return Self {
            size: 0,
            ftype, perm,
            uid: 0, gid: 0
        };
    }
}

// INTENSIONALLY FORCING INTERIOR MUTABILITY
pub trait VirtFNode: Send + Sync {
    fn meta(&self) -> FMeta;
    fn read(&self, buf: &mut [u8], offset: u64) -> bool;
    fn write(&self, buf: &[u8], offset: u64) -> bool;
    fn truncate(&self, size: u64) -> bool;
    fn list(&self) -> Option<Vec<String>>;
    fn walk(&self, name: &str) -> Option<Arc<dyn VirtFNode>>;
    fn create(&self, name: &str, node: Arc<dyn VirtFNode>) -> bool;
    fn remove(&self, name: &str) -> bool;
}