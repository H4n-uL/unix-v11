#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Kargs {
    pub kernel: KernelInfo,
    pub sys: SysInfo,
    pub kbase: usize,
    pub stack_base: usize
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SysInfo {
    pub layout_ptr: usize,
    pub layout_len: usize,
    pub acpi_ptr: usize,
    pub dtb_ptr: usize,
    pub disk_uuid: [u8; 16]
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KernelInfo {
    pub size: usize,
    pub ep: usize,
    pub text_ptr: usize,
    pub text_len: usize,
    pub rela_ptr: usize,
    pub rela_len: usize
}

#[repr(C)]
pub struct RelaEntry {
    pub offset: usize,
    pub info: usize,
    pub addend: isize
}