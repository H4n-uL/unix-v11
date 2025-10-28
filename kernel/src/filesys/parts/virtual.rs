use crate::filesys::{parts::Partition, vfn::VirtFNode, VirtDir};

use alloc::sync::Arc;

pub struct VirtPart {
    root: Arc<dyn VirtFNode>
}

impl VirtPart {
    pub fn new() -> Self {
        return Self {
            root: Arc::new(VirtDir::new())
        };
    }
}

impl Partition for VirtPart {
    fn root(&self) -> Arc<dyn VirtFNode> {
        return self.root.clone();
    }
}
