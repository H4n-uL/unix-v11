use core::sync::atomic::{AtomicUsize, Ordering as AtomOrd};
use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use spin::RwLock;

use crate::ram::mutex::IntRwLock;

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
    pub seg_ptr: usize,
    pub seg_len: usize,
    pub rela_ptr: usize,
    pub rela_len: usize
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

pub struct ApList {
    bitmap: IntRwLock<RwLock<()>, Vec<usize>>,
    phys2virt: IntRwLock<RwLock<()>, BTreeMap<usize, usize>>
}

impl ApList {
    pub const fn new() -> Self {
        return Self {
            bitmap: IntRwLock::new(Vec::new()),
            phys2virt: IntRwLock::new(BTreeMap::new())
        };
    }

    pub fn virtid_self(&self) -> usize {
        return *self.phys2virt.read()
            .get(&crate::arch::phys_id())
            .unwrap_or(&0);
    }

    pub fn assign(&self) -> usize {
        let physid = crate::arch::phys_id();
        let mut virtid = physid;
        let mut bm = self.bitmap.write();

        for (i, word) in bm.iter_mut().enumerate() {
            if *word != usize::MAX {
                let bit = (!*word).trailing_zeros() as usize;
                *word |= 1 << bit;
                virtid = i * usize::BITS as usize + bit;
                break;
            }
        }

        bm.push(1);
        self.phys2virt.write().insert(physid, virtid);
        return virtid;
    }

    pub fn release(&self, vid: usize) {
        let mut bm = self.bitmap.write();
        self.phys2virt.write().retain(|_, &mut v| v != vid);

        if (vid / usize::BITS as usize) < bm.len() {
            bm[vid / usize::BITS as usize] &= !(1 << (vid % usize::BITS as usize));
        }
        if bm.last() == Some(&0) {
            bm.pop();
        }
    }
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
    ElfSegments     = 0x7f454c46,
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
pub static AP_LIST: ApList = ApList::new();

impl KernelInfo {
    pub const fn empty() -> Self {
        Self {
            size: 0, ep: 0,
            seg_ptr: 0, seg_len: 0,
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

pub fn elf_segments<'a>() -> &'a [Segment] {
    let kinfo = KINFO.read();
    return unsafe { core::slice::from_raw_parts(kinfo.seg_ptr as *const Segment, kinfo.seg_len) };
}

pub fn set_kargs(kargs: Kargs) {
    KINFO.write().clone_from(&kargs.kernel);
    SYSINFO.write().clone_from(&kargs.sys);
    KBASE.store(kargs.kbase, AtomOrd::Relaxed);
}
