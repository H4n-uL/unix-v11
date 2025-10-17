use crate::filesys::{parts::Partition, vfn::VirtFNode, VirtDirectory};

use alloc::sync::Arc;

pub struct VirtPart {
    root: Arc<dyn VirtFNode>
}

impl VirtPart {
    pub fn new() -> Self {
        return Self {
            root: Arc::new(VirtDirectory::new())
        };
    }
}

impl Partition for VirtPart {
    fn root(&self) -> Arc<dyn VirtFNode> {
        return self.root.clone();
    }
}
