pub mod fat;
pub mod vpart;

use crate::filesys::vfn::VirtFNode;

use alloc::sync::Arc;

pub trait Partition: Send + Sync {
    fn root(self: Arc<Self>) -> Arc<dyn VirtFNode>;
}
