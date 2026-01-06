use crate::ram::STACK_SIZE;

use core::sync::atomic::{AtomicUsize, Ordering as AtomOrd};
use spin::RwLock;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Kargs {
    pub kernel: KernelInfo,
    pub sys: SysInfo,
    pub kbase: usize
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
#[derive(Clone, Copy, Debug)]
pub struct SysInfo {
    pub layout_ptr: usize,
    pub layout_len: usize,
    pub acpi_ptr: usize,
    pub dtb_ptr: usize,
    pub disk_uuid: [u8; 16]
}

#[repr(C)]
pub struct RelaEntry {
    pub offset: usize,
    pub info: usize,
    pub addend: isize
}

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
    Reclaimable     = 0xb6876800,
    UserPTable      = 0xba9b4000,
    Kernel          = 0xffffffff
}

pub const RECLAMABLE: &[RAMType] = &[
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

pub static KINFO: RwLock<KernelInfo> = RwLock::new(KernelInfo::empty());
pub static SYSINFO: RwLock<SysInfo> = RwLock::new(SysInfo::empty());
pub static KBASE: AtomicUsize = AtomicUsize::new(0);
pub static APID: AtomicUsize = AtomicUsize::new(0);

impl KernelInfo {
    pub const fn empty() -> Self {
        Self {
            size: 0, ep: 0,
            text_ptr: 0, text_len: 0,
            rela_ptr: 0, rela_len: 0
        }
    }
}

impl SysInfo {
    pub const fn empty() -> Self {
        Self {
            layout_ptr: 0,
            layout_len: 0,
            acpi_ptr: 0,
            dtb_ptr: 0,
            disk_uuid: [0; 16]
        }
    }
}

pub fn efi_ram_layout<'a>() -> &'a [RAMDescriptor] {
    let sys = SYSINFO.read();
    return unsafe { core::slice::from_raw_parts(sys.layout_ptr as *const RAMDescriptor, sys.layout_len) };
}

pub fn efi_ram_layout_mut<'a>() -> &'a mut [RAMDescriptor] {
    let sys = SYSINFO.read();
    return unsafe { core::slice::from_raw_parts_mut(sys.layout_ptr as *mut RAMDescriptor, sys.layout_len) };
}

pub fn set_kargs(kargs: Kargs) {
    KINFO.write().clone_from(&kargs.kernel);
    SYSINFO.write().clone_from(&kargs.sys);
    KBASE.store(kargs.kbase, AtomOrd::SeqCst);
}

pub fn ap_vid() -> usize {
    let sp = crate::arch::stack_ptr() as usize;
    if sp >> (usize::BITS - 1) == 0 { // if sp is lo-half
        return 0; // can be assumed as BSP
    }
    return (0usize.wrapping_sub(sp) / (STACK_SIZE << 1)) - 1;
}
