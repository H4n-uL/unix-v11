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
    layout_ptr: *const RAMDescriptor,
    layout_len: usize,
    pub acpi_rsdp_ptr: usize,
    pub stack_base: usize,
    pub kernel_base: usize,
    pub kernel_size: usize
}

const PAGE_4KIB: usize = 0x1000;

#[allow(unused)]
pub mod ramtype {
    pub const RESERVED             : u32 = 0x00;
    pub const LOADER_CODE          : u32 = 0x01;
    pub const LOADER_DATA          : u32 = 0x02;
    pub const BOOT_SERVICES_CODE   : u32 = 0x03;
    pub const BOOT_SERVICES_DATA   : u32 = 0x04;
    pub const RUNTIME_SERVICES_CODE: u32 = 0x05;
    pub const RUNTIME_SERVICES_DATA: u32 = 0x06;
    pub const CONVENTIONAL         : u32 = 0x07;
    pub const UNUSABLE             : u32 = 0x08;
    pub const ACPI_RECLAIM         : u32 = 0x09;
    pub const ACPI_NON_VOLATILE    : u32 = 0x0a;
    pub const MMIO                 : u32 = 0x0b;
    pub const MMIO_PORT_SPACE      : u32 = 0x0c;
    pub const PAL_CODE             : u32 = 0x0d;
    pub const PERSISTENT_MEMORY    : u32 = 0x0e;
    pub const UNACCEPTED           : u32 = 0x0f;
    pub const MAX                  : u32 = 0x10;

    // ...

    pub const LAYOUT_SELF          : u32 = 0x4452414d;
    pub const KERNEL               : u32 = 0x554e4958;
}

impl Ember {
    pub fn efi_ram_layout<'a>(&self) -> &'a [RAMDescriptor] {
        return unsafe { core::slice::from_raw_parts(self.layout_ptr, self.layout_len) };
    }

    fn efi_ram_layout_mut<'a>(&mut self) -> &'a mut [RAMDescriptor] {
        return unsafe { core::slice::from_raw_parts_mut(self.layout_ptr as *mut RAMDescriptor, self.layout_len) };
    }

    pub fn protect(&mut self) {
        let layout_start = self.layout_ptr as u64;
        let layout_end = layout_start + (self.layout_len * size_of::<RAMDescriptor>()) as u64;

        let kernel_start = self.kernel_base as u64;
        let kernel_end = (self.kernel_base + self.kernel_size) as u64;

        self.efi_ram_layout_mut().iter_mut().for_each(|desc| {
            let desc_start = desc.phys_start;
            let desc_end = desc.phys_start + desc.page_count * PAGE_4KIB as u64;
            if kernel_start < desc_end && kernel_end > desc_start { desc.ty = ramtype::KERNEL; }
            if layout_start < desc_end && layout_end > desc_start { desc.ty = ramtype::LAYOUT_SELF; }
        });
    }

    pub fn sort_ram_layout(&mut self) {
        use crate::sort::HeaplessSort;
        self.efi_ram_layout_mut()
            .sort_noheap_by(|a, b| a.phys_start.cmp(&b.phys_start));
    }

    pub fn set_new_stack_base(&mut self, stack_base: usize) {
        self.stack_base = stack_base;
    }
}