pub mod fat32;
pub mod r#virtual;

use crate::filesys::vfn::VirtFNode;

use alloc::sync::Arc;

pub trait Partition: Send + Sync {
    fn root(&self) -> Arc<dyn VirtFNode>;
}
