use spin::{Mutex, MutexGuard};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RAMDescriptor {
    pub ty: RAMType,
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
    pub layout_ptr: usize,
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

const PAGE_4KIB: usize = 0x1000;

#[allow(unused)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum RAMType {
    Reserved        = 0x00,
    LoaderCode      = 0x01,
    LoaderData      = 0x02,
    BootSvcCode     = 0x03,
    BootSvcData     = 0x04,
    RtSvcCode       = 0x05,
    RtSvcData       = 0x06,
    Conv            = 0x07,
    Unusable        = 0x08,
    ACPIReclaim     = 0x09,
    ACPINonVolatile = 0x0a,
    MMIO            = 0x0b,
    MMIOPortSpace   = 0x0c,
    PALCode         = 0x0d,
    PersistentRAM   = 0x0e,
    Unaccepted      = 0x0f,
    Max             = 0x10,

    // ...

    KernelData      = 0x44415441,
    EfiRamLayout    = 0x524c594f,
    KernelPTable    = 0x929b4000,
    UserPTable      = 0xba9b4000,
    Kernel          = 0xffffffff
}

const RECLAMABLE: &[RAMType] = &[
    RAMType::LoaderCode,
    RAMType::LoaderData,
    RAMType::BootSvcCode,
    RAMType::BootSvcData
];

pub const NON_RAM: &[RAMType] = &[
    RAMType::Reserved,
    RAMType::MMIO,
    RAMType::MMIOPortSpace
];

pub struct SysInfoMutex(Mutex<SysInfo>);
pub static SYS_INFO: SysInfoMutex = SysInfoMutex::new();

unsafe impl Send for SysInfo {}
unsafe impl Sync for SysInfo {}
unsafe impl Send for KernelInfo {}
unsafe impl Sync for KernelInfo {}

impl KernelInfo {
    pub const fn empty() -> Self {
        Self {
            base: 0, size: 0,
            ep: 0, text_ptr: 0, text_len: 0,
            rela_ptr: 0, rela_len: 0
        }
    }
}

impl SysInfo {
    pub const fn empty() -> Self {
        Self {
            kernel: KernelInfo::empty(),
            stack_base: 0,
            layout_ptr: 0,
            layout_len: 0,
            acpi_ptr: 0,
            dtb_ptr: 0,
            disk_uuid: [0; 16]
        }
    }

    pub fn init(&mut self, param: Self) {
        *self = param;

        let kernel_start = self.kernel.base as u64;
        let kernel_end = (self.kernel.base + self.kernel.size) as u64;
        let layout_start = self.layout_ptr as u64;
        let layout_end = unsafe { (self.layout_ptr as *const RAMDescriptor).add(self.layout_len) } as u64;

        self.efi_ram_layout_mut().iter_mut().for_each(|desc| {
            let desc_start = desc.phys_start;
            let desc_end = desc.phys_start + desc.page_count * PAGE_4KIB as u64;

            if kernel_start < desc_end && kernel_end > desc_start {
                desc.ty = RAMType::Kernel;
            }
            if layout_start < desc_end && layout_end > desc_start {
                desc.ty = RAMType::EfiRamLayout;
            }

            #[cfg(target_arch = "x86_64")]
            if desc.phys_start < 0x100000 {
                desc.ty = RAMType::Reserved;
            }

            if RECLAMABLE.contains(&desc.ty) {
                desc.ty = RAMType::Conv;
            }
        });
    }

    pub fn efi_ram_layout<'a>(&self) -> &'a [RAMDescriptor] {
        return unsafe { core::slice::from_raw_parts(self.layout_ptr as *const RAMDescriptor, self.layout_len) };
    }

    pub fn efi_ram_layout_mut<'a>(&mut self) -> &'a mut [RAMDescriptor] {
        return unsafe { core::slice::from_raw_parts_mut(self.layout_ptr as *mut RAMDescriptor, self.layout_len) };
    }

    pub fn set_new_stack_base(&mut self, stack_base: usize) {
        self.stack_base = stack_base;
    }
}

impl SysInfoMutex {
    pub const fn new() -> Self {
        Self(Mutex::new(SysInfo::empty()))
    }

    pub fn init(&self, param: SysInfo) {
        self.0.lock().init(param);
    }

    pub fn lock(&'_ self) -> MutexGuard<'_, SysInfo> {
        return self.0.lock();
    }

    pub fn efi_ram_layout<'a>(&self) -> &'a [RAMDescriptor] {
        return self.0.lock().efi_ram_layout();
    }

    pub fn efi_ram_layout_mut<'a>(&self) -> &'a mut [RAMDescriptor] {
        return self.0.lock().efi_ram_layout_mut();
    }

    pub fn set_new_stack_base(&self, stack_base: usize) {
        self.0.lock().set_new_stack_base(stack_base);
    }
}
