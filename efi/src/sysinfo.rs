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

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SysInfo {
    pub kernel: KernelInfo,
    pub stack_base: usize,
    pub layout_ptr: *const RAMDescriptor,
    pub layout_len: usize,
    pub acpi_ptr: usize,
    pub dtb_ptr: usize,
    pub disk_uuid: [u8; 16]
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KernelInfo {
    pub base: usize,
    pub size: usize,
    pub ep: usize,
    pub text_ptr: usize,
    pub text_len: usize,
    pub rela_ptr: usize,
    pub rela_len: usize
}

#[repr(C)]
pub struct RelaEntry {
    pub offset: u64,
    pub info: u64,
    pub addend: u64
}
