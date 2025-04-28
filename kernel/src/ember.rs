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

const PAGE_4KIB: usize = 0x1000;
const SELF: u32 = 0xffffffff;

impl Ember {
    pub fn protect_layout(&mut self) {
        let layout_ptr = self.layout_ptr as usize;
        let layout_len_bytes = self.layout_len * size_of::<RAMDescriptor>();
        let layout_start = layout_ptr as u64;
        let layout_end = (layout_ptr + layout_len_bytes) as u64;

        self.efi_ram_layout_mut().iter_mut().for_each(|desc| {
            let desc_start = desc.phys_start;
            let desc_end = desc.phys_start + desc.page_count * PAGE_4KIB as u64;
            if layout_start < desc_end && layout_end > desc_start { desc.ty = SELF; }
        });
    }

    fn efi_ram_layout_mut<'a>(&self) -> &'a mut [RAMDescriptor] {
        return unsafe { core::slice::from_raw_parts_mut(self.layout_ptr as *mut RAMDescriptor, self.layout_len) };
    }

    pub fn efi_ram_layout<'a>(&self) -> &'a [RAMDescriptor] {
        return unsafe { core::slice::from_raw_parts(self.layout_ptr, self.layout_len) };
    }

    pub fn set_new_stack_base(&mut self, stack_base: usize) {
        self.stack_base = stack_base;
    }
}