use crate::{
    arch::mmu::flags,
    device::block::{BlockDevice, BLOCK_DEVICES},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}, PAGE_4KIB}
};
use super::PCI_DEVICES;
use alloc::{collections::btree_map::BTreeMap, format, string::String, sync::Arc};
use nvme_rs::{Allocator, Device};
use spin::Mutex;

pub struct NVMeAlloc;

impl Allocator for NVMeAlloc {
    unsafe fn allocate(&self, size: usize) -> usize {
        return PHYS_ALLOC.alloc(AllocParams::new(size)).unwrap().addr();
    }

    unsafe fn deallocate(&self, addr: usize, size: usize) {
        unsafe { PHYS_ALLOC.free_raw(addr as *mut u8, size); }
    }

    fn translate(&self, addr: usize) -> usize { addr }
}

pub struct NVMeBlockDevice {
    dev: Arc<Device<NVMeAlloc>>,
    devid: usize,
    nsid: u32
}

impl NVMeBlockDevice {
    pub fn new(dev: Arc<Device<NVMeAlloc>>, devid: usize, nsid: u32) -> Self {
        Self { dev, devid, nsid }
    }

    pub fn devid(&self) -> usize { self.devid }
    pub fn nsid(&self) -> u32 { self.nsid }
}

impl BlockDevice for NVMeBlockDevice {
    fn block_size(&self) -> u64 {
        let namespace = self.dev.get_ns(self.nsid);
        return namespace.map_or(0, |namespace| namespace.block_size());
    }

    fn block_count(&self) -> u64 {
        let namespace = self.dev.get_ns(self.nsid);
        return namespace.map_or(0, |namespace| namespace.block_count());
    }

    fn read(&self, buffer: &mut [u8], lba: u64) -> Result<(), String> {
        let ns = self.dev.get_ns(self.nsid)
            .ok_or_else(|| String::from("Invalid namespace"))?;

        return ns.read(lba, buffer).map_err(|e|
            format!("NVMe read error: {}", e)
        );
    }

    fn write(&self, buffer: &[u8], lba: u64) -> Result<(), String> {
        let ns = self.dev.get_ns(self.nsid)
            .ok_or_else(|| String::from("Invalid namespace"))?;

        return ns.write(lba, buffer).map_err(|e|
            format!("NVMe write error: {}", e)
        );
    }
}

pub static NVME_DEV: Mutex<BTreeMap<usize, Arc<Device<NVMeAlloc>>>> = Mutex::new(BTreeMap::new());

pub fn init_nvme() {
    let mut nvme_devices = NVME_DEV.lock();
    let mut block_devices = BLOCK_DEVICES.lock();
    for pci_dev in PCI_DEVICES.lock().iter().filter(|&dev| dev.is_nvme()) {
        let base = pci_dev.bar(0).unwrap() as usize;
        let mmio_addr = if (base & 0b110) == 0b100 {
            ((pci_dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
        } else { base & !0b11 };

        let devid = nvme_devices.len();
        GLACIER.map_range(mmio_addr, mmio_addr, PAGE_4KIB * 2, flags::D_RW);
        let nvme_arc = Arc::new(Device::init(mmio_addr, NVMeAlloc).unwrap());
        for ns in nvme_arc.list_namespaces() {
            block_devices.push(Arc::new(NVMeBlockDevice::new(nvme_arc.clone(), devid, ns)));
        }
        nvme_devices.insert(devid, nvme_arc.clone());
    }
}