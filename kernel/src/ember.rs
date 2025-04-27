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
pub struct Ember {
    pub layout_ptr: *const RAMDescriptor,
    pub layout_len: usize,
    pub acpi_rsdp_ptr: usize,
    pub stack_base: usize,
    pub kernel_base: usize,
    pub kernel_size: usize
}

impl Ember {
    #[inline(always)]
    pub fn efi_ram_layout<'a>(&self) -> &'a [RAMDescriptor] {
        return unsafe { core::slice::from_raw_parts(self.layout_ptr, self.layout_len) };
    }

    pub fn set_new_stack_base(&mut self, stack_base: usize) {
        self.stack_base = stack_base;
    }
}