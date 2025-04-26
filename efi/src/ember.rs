#[repr(C)]
pub struct Ember {
    pub layout_ptr: *const RAMDescriptor,
    pub layout_len: usize,
    pub kernel_size: usize,
    pub stack_ptr: usize
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMDescriptor {
    pub ty: u32,
    pub reserved: u32,
    pub phys_start: u64,
    pub virt_start: u64,
    pub page_count: u64,
    pub attr: u64,
    pub padding: u64
}