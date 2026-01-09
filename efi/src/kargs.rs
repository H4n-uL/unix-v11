#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Kargs {
    pub kernel: KernelInfo,
    pub sys: SysInfo,
    pub kbase: usize
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
    pub seg_ptr: usize,
    pub seg_len: usize,
    pub dyn_ptr: usize,
    pub dyn_len: usize
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub ptr: usize,
    pub len: usize,
    pub flags: u32,
    pub align: u32
}

#[repr(C)]
pub struct DynEntry {
    pub tag: usize,
    pub val: usize
}

#[repr(C)]
pub struct SymEntry {
    pub name: u32,
    pub info: u8,
    pub other: u8,
    pub shndx: u16,
    pub value: usize,
    pub size: usize
}

#[repr(C)]
pub struct RelaEntry {
    pub offset: usize,
    pub info: usize,
    pub addend: isize
}
